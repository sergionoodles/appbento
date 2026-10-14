//! Install a signed update alongside the running image and hand off to it.
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::self_launcher::LauncherUpdate;

const STARTUP_READY_ENV: &str = "APPBENTO_SELF_UPDATE_READY";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(100);

struct TemporaryFile {
    path: PathBuf,
    keep: bool,
}

impl TemporaryFile {
    fn create(path: &Path) -> Result<Self, String> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("Cannot create {}: {error}", path.display()))?;
        Ok(Self {
            path: path.to_owned(),
            keep: false,
        })
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("Cannot clean up {}: {error}", self.path.display()),
        }
    }
}

fn updated_path(current: &Path, version: &str) -> Result<PathBuf, String> {
    if version.is_empty()
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
    {
        return Err("The update version cannot be used in an AppImage filename.".into());
    }
    let path = current.with_file_name(format!(
        "AppBento_{version}_{}.AppImage",
        std::env::consts::ARCH,
    ));
    if path == current {
        return Err(
            "The new AppImage must have a different filename from the running image.".into(),
        );
    }
    Ok(path)
}

fn install_image(
    current: &Path,
    staging_path: &Path,
    version: &str,
    install: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<TemporaryFile, String> {
    let destination = updated_path(current, version)?;
    let staging = TemporaryFile::create(staging_path)?;
    install(&staging.path)?;
    // Publishing without replacement protects an existing file with the same release name.
    fs::hard_link(&staging.path, &destination)
        .map_err(|error| format!("Cannot install {}: {error}", destination.display()))?;
    Ok(TemporaryFile {
        path: destination,
        keep: false,
    })
}

fn activate_image(
    current: &Path,
    mut installed: TemporaryFile,
    update_launcher: impl FnOnce(&Path) -> Result<Option<LauncherUpdate>, String>,
    launch: impl FnOnce(&Path) -> Result<(), String>,
    restore_library_launchers: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let launcher_update = update_launcher(&installed.path)?;
    if let Err(error) = launch(&installed.path) {
        let library_rollback = restore_library_launchers();
        if library_rollback.is_err() {
            installed.keep = true;
        }
        if let Some(launcher_update) = launcher_update {
            if let Err(rollback_error) = launcher_update.rollback() {
                // Keep the new file if the launcher could still point to it.
                installed.keep = true;
                return Err(format!(
                    "{error}. Could not restore the launcher: {rollback_error}"
                ));
            }
        }
        if let Err(rollback_error) = library_rollback {
            return Err(format!(
                "{error}. Could not restore library launchers: {rollback_error}"
            ));
        }
        return Err(error);
    }
    installed.keep = true;
    if let Err(error) = fs::remove_file(current) {
        eprintln!(
            "Update started; could not remove old AppImage {}: {error}",
            current.display()
        );
    }
    Ok(())
}

pub(crate) fn install_and_launch(
    current: &Path,
    staging_path: &Path,
    version: &str,
    install: impl FnOnce(&Path) -> Result<(), String>,
    update_launcher: impl FnOnce(&Path) -> Result<Option<LauncherUpdate>, String>,
    restore_library_launchers: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let installed = install_image(current, staging_path, version, install)?;
    let ready_path = staging_path.with_extension("ready");
    activate_image(
        current,
        installed,
        update_launcher,
        |executable| launch_and_wait(executable, &ready_path),
        restore_library_launchers,
    )
}

fn launch_and_wait(executable: &Path, ready_path: &Path) -> Result<(), String> {
    let ready = TemporaryFile::create(ready_path)?;
    let mut child = Command::new(executable)
        .env(STARTUP_READY_ENV, &ready.path)
        .env_remove("APPIMAGE")
        .env_remove("APPDIR")
        .stdin(Stdio::null())
        .spawn()
        .map_err(|error| format!("Cannot launch the updated AppImage: {error}"))?;
    let started = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                break Err(format!(
                    "The updated AppImage exited during startup: {status}"
                ))
            }
            Ok(None) => {}
            Err(error) => break Err(format!("Cannot monitor the updated AppImage: {error}")),
        }
        match fs::read(&ready.path) {
            Ok(bytes) if bytes == b"ready" => break Ok(()),
            Ok(_) => {}
            Err(error) => break Err(format!("Cannot check update startup: {error}")),
        }
        if started.elapsed() >= STARTUP_TIMEOUT {
            break Err("The updated AppImage did not finish starting within 30 seconds.".into());
        }
        std::thread::sleep(STARTUP_POLL_INTERVAL);
    };
    if result.is_err() {
        if let Err(error) = child.kill() {
            eprintln!("Cannot stop the failed update process: {error}");
        }
        if let Err(error) = child.wait() {
            eprintln!("Cannot wait for the failed update process: {error}");
        }
    }
    result
}

pub(crate) fn notify_startup() -> Result<(), String> {
    if let Some(path) = std::env::var_os(STARTUP_READY_ENV) {
        // Library desktop entries invoke AppBento to apply each app's launch policy.
        // Reuse their normal migration before the parent removes its old executable.
        crate::apps::lifecycle()?.list()?;
        fs::write(path, b"ready").map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "self_update_installation_tests.rs"]
mod tests;
