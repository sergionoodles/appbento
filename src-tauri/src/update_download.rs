//! Stream update files without executing them; content checks catch same-size changes.
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::github::ResolvedUpdate;
use crate::models::{InstalledApp, UpdateCheckResult, UpdateInfo, UpdateStatus};
use crate::{lifecycle::Lifecycle, persistence, sandbox};

const MAX_DOWNLOAD_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const DIRECT_UPDATE_LABEL: &str = "Updated file";

async fn transfer(
    url: &str,
    destination: Option<&mut tokio::fs::File>,
    expected_size: Option<u64>,
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
    consume_response(response, destination, expected_size).await
}

async fn consume_response(
    mut response: reqwest::Response,
    mut destination: Option<&mut tokio::fs::File>,
    expected_size: Option<u64>,
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
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        received = received
            .checked_add(chunk.len() as u64)
            .ok_or("download size overflow")?;
        if received > MAX_DOWNLOAD_BYTES {
            return Err("update download exceeds the 4 GiB limit".into());
        }
        digest.update(&chunk);
        if let Some(file) = destination.as_deref_mut() {
            file.write_all(&chunk)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    if received == 0 || expected_size.is_some_and(|size| received != size) {
        return Err("update download is empty or its size does not match the release".into());
    }
    Ok(format!("{:x}", digest.finalize()))
}

async fn installed_digest(app: &InstalledApp) -> Result<String, String> {
    let path = PathBuf::from(&app.file_path);
    tokio::task::spawn_blocking(move || sandbox::appimage_digest(&path))
        .await
        .map_err(|error| error.to_string())?
}

pub async fn check_content(
    app: &InstalledApp,
    resolved: ResolvedUpdate,
) -> Result<UpdateCheckResult, String> {
    let current_digest = installed_digest(app).await?;
    let remote_digest = transfer(&resolved.download_url, None, resolved.size_bytes).await?;
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
        update,
        error: None,
    }
}

fn reserve_download() -> Result<(PathBuf, std::fs::File), String> {
    let directory = persistence::data_path("update-downloads")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    persistence::create_temporary(&directory)
}

pub async fn install(app: InstalledApp, lifecycle: Lifecycle) -> Result<InstalledApp, String> {
    let config = app
        .update_config
        .as_ref()
        .ok_or("configure an update source first")?;
    let expected_digest = installed_digest(&app).await?;
    let resolved = crate::update_sources::resolve(config).await?;
    let is_direct = matches!(config, crate::models::UpdateConfig::DirectUrl { .. });
    if !is_direct {
        let result = crate::updates::resolve_update(
            &app,
            ResolvedUpdate {
                latest: resolved.latest.clone(),
                download_url: resolved.download_url.clone(),
                asset_name: resolved.asset_name.clone(),
                published_at: resolved.published_at,
                size_bytes: resolved.size_bytes,
            },
        );
        if result.update.is_none() {
            return Err(result
                .error
                .unwrap_or_else(|| "no newer release is available".into()));
        }
    }
    let (path, file) = tokio::task::spawn_blocking(reserve_download)
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
    let digest = transfer(&resolved.download_url, Some(file), resolved.size_bytes).await?;
    file.sync_all().await.map_err(|error| error.to_string())?;
    if digest == expected_digest {
        return Err("the downloaded AppImage is identical to the installed file".into());
    }
    let app = app.clone();
    let path = path.to_owned();
    let expected_digest = expected_digest.to_owned();
    let latest = (!is_direct).then(|| resolved.latest.clone());
    let lifecycle = Lifecycle {
        library_path: lifecycle.library_path.clone(),
        managed_dir: lifecycle.managed_dir.clone(),
        desktop_dir: lifecycle.desktop_dir.clone(),
    };
    tokio::task::spawn_blocking(move || {
        lifecycle.replace_update(&app, &path, latest.as_deref(), &expected_digest)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

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
        )
        .await
        .unwrap();
        file.sync_all().await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"appimage bytes");
        assert_eq!(digest, format!("{:x}", Sha256::digest(b"appimage bytes")));
        let changed = consume_response(response(b"changed! bytes", 14).await, None, None)
            .await
            .unwrap();
        assert_ne!(digest, changed);
    }

    #[tokio::test]
    async fn rejects_empty_truncated_mismatched_and_oversized_downloads() {
        assert!(consume_response(response(b"", 0).await, None, None)
            .await
            .is_err());
        assert!(consume_response(response(b"short", 10).await, None, None)
            .await
            .is_err());
        assert!(consume_response(response(b"bytes", 5).await, None, Some(6))
            .await
            .is_err());
        assert!(
            consume_response(response(b"", MAX_DOWNLOAD_BYTES + 1).await, None, None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn refuses_insecure_downloads_without_network() {
        for url in [
            "http://example.com/app",
            "https://user:secret@example.com/app",
            "file:///tmp/app",
        ] {
            assert!(transfer(url, None, None).await.is_err());
        }
    }
}
