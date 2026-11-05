//! Stream update files without executing them; content checks catch same-size changes.
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::github::{ExpectedChecksum, ResolvedUpdate};
use crate::models::{InstalledApp, UpdateCheckResult, UpdateInfo, UpdateStatus};
use crate::update_checksum::ChecksumVerifier;
use crate::{lifecycle::Lifecycle, persistence};

const MAX_DOWNLOAD_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const DIRECT_UPDATE_LABEL: &str = "Updated file";

pub(crate) async fn transfer(
    url: &str,
    destination: Option<&mut tokio::fs::File>,
    expected_size: Option<u64>,
    expected_checksum: Option<&ExpectedChecksum>,
) -> Result<String, String> {
    let url = reqwest::Url::parse(url).map_err(|error| error.to_string())?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("update downloads must use HTTPS without credentials".into());
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .user_agent("appbento/0.1")
        .connect_timeout(CONNECTION_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("too many redirects");
            }
            if crate::github::validate_https_url(attempt.url().as_str()).is_err() {
                return attempt.error("unsafe update redirect");
            }
            attempt.follow()
        }))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    consume_response(response, destination, expected_size, expected_checksum).await
}

async fn consume_response(
    mut response: reqwest::Response,
    mut destination: Option<&mut tokio::fs::File>,
    expected_size: Option<u64>,
    expected_checksum: Option<&ExpectedChecksum>,
) -> Result<String, String> {
    if response
        .content_length()
        .is_some_and(|size| size > MAX_DOWNLOAD_BYTES)
        || expected_size.is_some_and(|size| size > MAX_DOWNLOAD_BYTES)
    {
        return Err("update download exceeds the 4 GiB limit".into());
    }
    let mut received = 0_u64;
    let mut digest = Sha256::new();
    let mut verifier = expected_checksum.map(ChecksumVerifier::new);
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        received = received
            .checked_add(chunk.len() as u64)
            .ok_or("download size overflow")?;
        if received > MAX_DOWNLOAD_BYTES {
            return Err("update download exceeds the 4 GiB limit".into());
        }
        digest.update(&chunk);
        if let Some(verifier) = verifier.as_mut() {
            verifier.update(&chunk);
        }
        if let Some(file) = destination.as_deref_mut() {
            file.write_all(&chunk)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    if received == 0 || expected_size.is_some_and(|size| received != size) {
        return Err("update download is empty or its size does not match the release".into());
    }
    if let Some(verifier) = verifier {
        verifier.verify()?;
    }
    Ok(format!("{:x}", digest.finalize()))
}

async fn installed_digest(app: &InstalledApp) -> Result<String, String> {
    let path = PathBuf::from(&app.file_path);
    tokio::task::spawn_blocking(move || crate::lifecycle::app_file::appimage_digest(&path))
        .await
        .map_err(|error| error.to_string())?
}

/// Compares the published SHA-1 with the installed file; only the control-file header was fetched.
pub async fn check_zsync(
    app: &InstalledApp,
    resolved: ResolvedUpdate,
) -> Result<UpdateCheckResult, String> {
    app.require_appimage_update()?;
    let Some(ExpectedChecksum::Sha1Hex(published)) = resolved.checksum.clone() else {
        return Err("the zsync control file has no SHA-1".into());
    };
    let path = PathBuf::from(&app.file_path);
    let installed =
        tokio::task::spawn_blocking(move || crate::update_checksum::sha1_hex_of_file(&path))
            .await
            .map_err(|error| error.to_string())??;
    Ok(content_result(
        app,
        resolved,
        installed.eq_ignore_ascii_case(&published),
    ))
}

pub async fn check_content(
    app: &InstalledApp,
    resolved: ResolvedUpdate,
) -> Result<UpdateCheckResult, String> {
    app.require_appimage_update()?;
    let current_digest = installed_digest(app).await?;
    let remote_digest = transfer(
        &resolved.download_url,
        None,
        resolved.size_bytes,
        resolved.checksum.as_ref(),
    )
    .await?;
    Ok(content_result(
        app,
        resolved,
        current_digest == remote_digest,
    ))
}

fn content_result(
    app: &InstalledApp,
    resolved: ResolvedUpdate,
    is_identical: bool,
) -> UpdateCheckResult {
    let status = if is_identical {
        UpdateStatus::UpToDate
    } else {
        UpdateStatus::Available
    };
    let update = (!is_identical).then(|| UpdateInfo {
        app_id: app.id.clone(),
        current: app.version_current.clone(),
        latest: DIRECT_UPDATE_LABEL.into(),
        download_url: resolved.download_url,
        asset_name: resolved.asset_name,
        published_at: resolved.published_at,
        size_bytes: resolved.size_bytes,
        status: status.clone(),
    });
    UpdateCheckResult {
        app_id: app.id.clone(),
        status,
        latest: None,
        update,
        error: None,
    }
}

fn reserve_download(directory: &Path) -> Result<(PathBuf, std::fs::File), String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    persistence::create_temporary(directory)
}

pub async fn install(app: InstalledApp, lifecycle: Lifecycle) -> Result<InstalledApp, String> {
    app.require_appimage_update()?;
    let config = app
        .update_config
        .as_ref()
        .ok_or("configure an update source first")?;
    let expected_digest = installed_digest(&app).await?;
    let resolved = crate::update_sources::resolve(config).await?;
    // These sources publish no comparable version, so the content decides.
    let is_direct = matches!(
        config,
        crate::models::UpdateConfig::DirectUrl { .. } | crate::models::UpdateConfig::Zsync { .. }
    );
    if !is_direct {
        let result = crate::updates::resolve_update(&app, resolved.clone());
        if result.update.is_none() {
            return Err(result
                .error
                .unwrap_or_else(|| "no newer release is available".into()));
        }
    }
    let managed_dir = lifecycle.managed_dir.clone();
    let (path, file) = tokio::task::spawn_blocking(move || reserve_download(&managed_dir))
        .await
        .map_err(|error| error.to_string())??;
    let mut file = tokio::fs::File::from_std(file);
    let result = install_download(
        &app,
        &lifecycle,
        &resolved,
        &path,
        &mut file,
        &expected_digest,
        is_direct,
    )
    .await;
    drop(file);
    let cleanup = tokio::fs::remove_file(&path).await;
    match (result, cleanup) {
        (Ok(app), Ok(())) => Ok(app),
        (Err(error), Ok(())) => Err(error),
        (result, Err(error)) => Err(format!(
            "{}; download cleanup failed: {error}",
            result.err().unwrap_or_else(|| "update installed".into())
        )),
    }
}

async fn install_download(
    app: &InstalledApp,
    lifecycle: &Lifecycle,
    resolved: &ResolvedUpdate,
    path: &Path,
    file: &mut tokio::fs::File,
    expected_digest: &str,
    is_direct: bool,
) -> Result<InstalledApp, String> {
    let digest = transfer(
        &resolved.download_url,
        Some(file),
        resolved.size_bytes,
        resolved.checksum.as_ref(),
    )
    .await?;
    file.sync_all().await.map_err(|error| error.to_string())?;
    if digest == expected_digest {
        return Err("the downloaded AppImage is identical to the installed file".into());
    }
    let app = app.clone();
    let path = path.to_owned();
    let expected_digest = expected_digest.to_owned();
    let latest = (!is_direct).then(|| resolved.latest.clone());
    let asset_name = if is_direct
        && !resolved
            .asset_name
            .to_ascii_lowercase()
            .ends_with(".appimage")
    {
        app.file_name.clone()
    } else {
        resolved.asset_name.clone()
    };
    let lifecycle = Lifecycle {
        library_path: lifecycle.library_path.clone(),
        managed_dir: lifecycle.managed_dir.clone(),
        desktop_dir: lifecycle.desktop_dir.clone(),
    };
    tokio::task::spawn_blocking(move || {
        lifecycle.replace_update(
            &app,
            &path,
            &asset_name,
            latest.as_deref(),
            &expected_digest,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn download_staging_uses_configured_managed_folder_without_overwriting_files() {
        let directory = persistence::tests::TemporaryDirectory::new();
        let managed = directory.0.join("custom-apps");
        let (first, first_file) = reserve_download(&managed).unwrap();
        let (second, second_file) = reserve_download(&managed).unwrap();

        assert_eq!(first.parent(), Some(managed.as_path()));
        assert_eq!(second.parent(), Some(managed.as_path()));
        assert_ne!(first, second);
        assert!(first.exists());
        assert!(second.exists());
        drop(first_file);
        drop(second_file);
    }

    async fn response(body: &[u8], advertised_size: u64) -> reqwest::Response {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_vec();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).unwrap() > 0);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {advertised_size}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        });
        reqwest::get(format!("http://{address}")).await.unwrap()
    }

    #[tokio::test]
    async fn streams_download_to_disk_and_hashes_content() {
        let directory = persistence::tests::TemporaryDirectory::new();
        let path = directory.0.join("download");
        let mut file = tokio::fs::File::create(&path).await.unwrap();
        let digest = consume_response(
            response(b"appimage bytes", 14).await,
            Some(&mut file),
            Some(14),
            None,
        )
        .await
        .unwrap();
        file.sync_all().await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"appimage bytes");
        assert_eq!(digest, format!("{:x}", Sha256::digest(b"appimage bytes")));
        let changed = consume_response(response(b"changed! bytes", 14).await, None, None, None)
            .await
            .unwrap();
        assert_ne!(digest, changed);
    }

    #[tokio::test]
    async fn rejects_empty_truncated_mismatched_and_oversized_downloads() {
        assert!(consume_response(response(b"", 0).await, None, None, None)
            .await
            .is_err());
        assert!(
            consume_response(response(b"short", 10).await, None, None, None)
                .await
                .is_err()
        );
        assert!(
            consume_response(response(b"bytes", 5).await, None, Some(6), None)
                .await
                .is_err()
        );
        assert!(consume_response(
            response(b"", MAX_DOWNLOAD_BYTES + 1).await,
            None,
            None,
            None
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn refuses_insecure_downloads_without_network() {
        for url in [
            "http://example.com/app",
            "https://user:secret@example.com/app",
            "file:///tmp/app",
        ] {
            assert!(transfer(url, None, None, None).await.is_err());
        }
    }

    #[tokio::test]
    async fn verifies_the_published_checksum_while_downloading() {
        let sha1 = "6dcd4ce23d88e2ee9568ba546c007c63d9131c1b";
        let matching = ExpectedChecksum::Sha1Hex(sha1.into());
        let digest = consume_response(response(b"A", 1).await, None, Some(1), Some(&matching))
            .await
            .unwrap();
        assert_eq!(digest, format!("{:x}", Sha256::digest(b"A")));
        let mismatch = consume_response(response(b"B", 1).await, None, Some(1), Some(&matching))
            .await
            .unwrap_err();
        assert!(mismatch.contains("checksum"));
        let sha512 = ExpectedChecksum::Sha512Base64(crate::update_checksum::base64_encode(
            &sha2::Sha512::digest(b"A"),
        ));
        assert!(
            consume_response(response(b"A", 1).await, None, None, Some(&sha512))
                .await
                .is_ok()
        );
        assert!(
            consume_response(response(b"B", 1).await, None, None, Some(&sha512))
                .await
                .is_err()
        );
    }

    fn installed(path: &Path) -> InstalledApp {
        InstalledApp {
            kind: crate::models::AppKind::AppImage,
            update_config: None,
            update_config_origin: None,
            id: path.to_string_lossy().into_owned(),
            name: "App".into(),
            file_path: path.to_string_lossy().into_owned(),
            source_path: None,
            file_name: "App.AppImage".into(),
            size_bytes: 0,
            executable: true,
            integrated: false,
            version_current: None,
            icon_path: None,
            desktop_id: None,
            launch_count: 0,
            last_launched: None,
            added_at: chrono::Utc::now(),
            file_created_at: None,
            launch_config: None,
            reviewed_digest: None,
        }
    }

    fn zsync_release(sha1: &str) -> ResolvedUpdate {
        ResolvedUpdate {
            latest: String::new(),
            download_url: "https://downloads.example/App.AppImage".into(),
            asset_name: "App.AppImage".into(),
            published_at: None,
            size_bytes: Some(10),
            checksum: Some(ExpectedChecksum::Sha1Hex(sha1.into())),
        }
    }

    #[tokio::test]
    async fn zsync_checks_compare_the_installed_sha1_with_the_control_file() {
        let directory = persistence::tests::TemporaryDirectory::new();
        let path = directory.0.join("App.AppImage");
        std::fs::write(&path, crate::elf_sections::synthetic_elf(None)).unwrap();
        let app = installed(&path);
        let installed_sha1 = crate::update_checksum::sha1_hex_of_file(&path).unwrap();

        let current = check_zsync(&app, zsync_release(&installed_sha1.to_uppercase()))
            .await
            .unwrap();
        assert_eq!(current.status, UpdateStatus::UpToDate);
        assert!(current.update.is_none());

        let changed = check_zsync(&app, zsync_release(&"0".repeat(40)))
            .await
            .unwrap();
        assert_eq!(changed.status, UpdateStatus::Available);
        let update = changed.update.unwrap();
        assert_eq!(update.latest, DIRECT_UPDATE_LABEL);
        assert_eq!(update.size_bytes, Some(10));

        let mut missing = zsync_release("");
        missing.checksum = None;
        assert!(check_zsync(&app, missing).await.is_err());
    }
}
