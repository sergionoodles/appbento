//! Download to private staging storage before the journaled managed-folder import.
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use crate::lifecycle::Lifecycle;
use crate::models::InstalledApp;
use crate::{github, persistence, update_download};

fn download_name(url: &reqwest::Url) -> String {
    let segment = url.path_segments().and_then(|segments| segments.last());
    let name = segment
        .filter(|name| name.to_ascii_lowercase().ends_with(".appimage"))
        .unwrap_or("Downloaded.AppImage");
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn reserve_directory(parent: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    loop {
        let directory = persistence::temporary_path(parent);
        match fs::DirBuilder::new().mode(0o700).create(&directory) {
            Ok(()) => return Ok(directory),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
}

pub async fn install(url: &str, lifecycle: Lifecycle) -> Result<InstalledApp, String> {
    let url = github::validate_https_url(url)?;
    let name = download_name(&url);
    let directory = tokio::task::spawn_blocking(|| {
        reserve_directory(&persistence::data_path("import-downloads")?)
    })
    .await
    .map_err(|error| error.to_string())??;
    let result = download_and_integrate(url.as_str(), directory.join(name), lifecycle).await;
    let cleanup = tokio::fs::remove_dir_all(&directory).await;
    match (result, cleanup) {
        (result, Ok(())) => result,
        (result, Err(error)) => Err(format!(
            "{}; download cleanup failed: {error}",
            result.err().unwrap_or_else(|| "AppImage added".into())
        )),
    }
}

async fn download_and_integrate(
    url: &str,
    path: PathBuf,
    lifecycle: Lifecycle,
) -> Result<InstalledApp, String> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .map_err(|error| error.to_string())?;
    update_download::transfer(url, Some(&mut file), None).await?;
    file.sync_all().await.map_err(|error| error.to_string())?;
    drop(file);
    tokio::task::spawn_blocking(move || lifecycle.integrate_download(&path))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_safe_names_and_handles_download_endpoints() {
        for (url, expected) in [
            (
                "https://example.org/Example.AppImage?token=abc",
                "Example.AppImage",
            ),
            ("https://example.org/My%20App.AppImage", "My_20App.AppImage"),
            ("https://example.org/download?id=1", "Downloaded.AppImage"),
            ("https://example.org/", "Downloaded.AppImage"),
        ] {
            assert_eq!(download_name(&reqwest::Url::parse(url).unwrap()), expected);
        }
    }

    #[tokio::test]
    async fn rejects_unsafe_urls_before_downloading() {
        let directory = persistence::tests::TemporaryDirectory::new();
        for url in [
            "http://example.org/app",
            "file:///tmp/app",
            "https://user:secret@example.org/app",
        ] {
            let lifecycle = Lifecycle {
                library_path: directory.0.join("library.json"),
                managed_dir: directory.0.join("managed"),
                desktop_dir: directory.0.join("desktop"),
            };
            assert!(install(url, lifecycle).await.is_err());
        }
        assert!(!directory.0.join("managed").exists());
    }
}
