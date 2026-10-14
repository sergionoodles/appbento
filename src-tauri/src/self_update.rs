use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
static PROBE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub struct SelfUpdateState {
    pending: Mutex<Option<PendingUpdate>>,
    pub is_installing: AtomicBool,
}

struct PendingUpdate {
    update: Update,
    staging_path: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
    download_url: String,
    install_disabled_reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    current_version: String,
    update: Option<UpdateInfo>,
    unavailable_reason: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum Progress {
    Downloading { downloaded: u64, total: Option<u64> },
    Installing,
    Restarting,
}

fn appimage_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.env().appimage.map(PathBuf::from).ok_or_else(|| {
        "Automatic updates are available when running the packaged AppBento AppImage.".into()
    })
}

fn validate_install_location(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("The AppImage must be a regular file. Download the update manually.".into());
    }
    let parent = path
        .parent()
        .ok_or("The AppImage has no parent directory.")?;
    let sequence = PROBE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let probe = parent.join(format!(
        ".appbento-update-check-{}-{sequence}",
        std::process::id()
    ));
    // Replacement needs directory write access, which file permission bits alone cannot establish.
    let file = OpenOptions::new().write(true).create_new(true).open(&probe)
        .map_err(|error| format!("Cannot update the AppImage in this folder: {error}. Move it to a writable folder or download the update manually."))?;
    drop(file);
    fs::remove_file(probe)
        .map_err(|error| format!("Cannot clean up the update permission check: {error}"))
}

#[tauri::command]
pub async fn self_update_check(
    app: AppHandle,
    state: State<'_, SelfUpdateState>,
) -> Result<CheckResult, String> {
    let current_version = app.package_info().version.to_string();
    let path = match appimage_path(&app) {
        Ok(path) => path,
        Err(reason) => {
            return Ok(CheckResult {
                current_version,
                update: None,
                unavailable_reason: Some(reason),
            })
        }
    };
    if state.is_installing.load(Ordering::Acquire) {
        return Err("An AppBento update is already being installed.".into());
    }
    let mut pending = state.pending.lock().await;
    let staging_path = path.with_file_name(format!(
        ".appbento-update-{}-{}.AppImage",
        std::process::id(),
        PROBE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    ));
    let update = app
        .updater_builder()
        .executable_path(&staging_path)
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| format!("Could not check for AppBento updates: {error}"))?;
    let info = update.as_ref().map(|update| UpdateInfo {
        version: update.version.clone(),
        notes: update.body.clone(),
        download_url: update.download_url.to_string(),
        install_disabled_reason: validate_install_location(&path).err(),
    });
    *pending = update.map(|update| PendingUpdate {
        update,
        staging_path,
    });
    Ok(CheckResult {
        current_version,
        update: info,
        unavailable_reason: None,
    })
}

struct InstallingGuard<'a>(&'a AtomicBool);

impl Drop for InstallingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub async fn self_update_install(
    app: AppHandle,
    state: State<'_, SelfUpdateState>,
    on_progress: Channel<Progress>,
) -> Result<(), String> {
    state
        .is_installing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "An AppBento update is already being installed.")?;
    let _guard = InstallingGuard(&state.is_installing);
    let path = appimage_path(&app)?;
    validate_install_location(&path)?;
    let mut pending = state.pending.lock().await;
    let pending_update = pending
        .as_ref()
        .ok_or("Check for an AppBento update before installing.")?;
    let mut update = pending_update.update.clone();
    let staging_path = pending_update.staging_path.clone();
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let mut downloaded = 0;
    let mut progress_error = None;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                if progress_error.is_none() {
                    progress_error = on_progress
                        .send(Progress::Downloading { downloaded, total })
                        .err();
                }
            },
            || {},
        )
        .await
        .map_err(|error| format!("Could not download or verify the AppBento update: {error}"))?;
    if let Some(error) = progress_error {
        return Err(format!("The update window disconnected: {error}"));
    }
    on_progress
        .send(Progress::Installing)
        .map_err(|error| error.to_string())?;
    tokio::task::spawn_blocking(move || {
        crate::self_update_installation::install_and_launch(
            &path,
            &staging_path,
            &update.version,
            |staging_path| {
                update.install(bytes).map_err(|error| error.to_string())?;
                fs::set_permissions(
                    staging_path,
                    fs::metadata(&path)
                        .map_err(|error| error.to_string())?
                        .permissions(),
                )
                .map_err(|error| error.to_string())
            },
            crate::self_launcher::update_executable,
            || crate::apps::lifecycle()?.list().map(|_| ()),
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| {
        format!(
            "Could not install the AppBento update: {error}. You can download it manually instead."
        )
    })?;
    *pending = None;
    if let Err(error) = on_progress.send(Progress::Restarting) {
        eprintln!("Update installed; could not report restart progress: {error}");
    }
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::tests::TemporaryDirectory;

    #[test]
    fn writable_install_location_preserves_image_and_leaves_no_probe() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("AppBento.AppImage");
        fs::write(&path, b"current image").unwrap();
        validate_install_location(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"current image");
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn missing_or_non_regular_images_cannot_be_replaced() {
        let directory = TemporaryDirectory::new();
        assert!(validate_install_location(&directory.0.join("missing")).is_err());
        assert!(validate_install_location(&directory.0).is_err());
    }

    #[test]
    fn failed_install_releases_busy_state() {
        let busy = AtomicBool::new(true);
        drop(InstallingGuard(&busy));
        assert!(!busy.load(Ordering::Acquire));
    }
}
