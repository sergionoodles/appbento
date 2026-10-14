//! Register AppBento itself without adding it to its AppImage library.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{lifecycle::desktop, persistence};

const ENTRY_NAME: &str = "appbento.desktop";
const OWNERSHIP_MARKER: &str = "X-AppBento-Self=true";
const ICON: &[u8] = include_bytes!("../icons/128x128.png");

#[derive(Default, Deserialize, Serialize)]
struct Preferences {
    dismissed: bool,
}

#[derive(Debug, Serialize)]
pub struct LauncherStatus {
    installed: bool,
    dismissed: bool,
}

struct Launcher {
    preferences: PathBuf,
    entry: PathBuf,
    icon: PathBuf,
}

impl Launcher {
    fn current() -> Result<Self, String> {
        Ok(Self {
            preferences: persistence::data_path("self-launcher.json")?,
            entry: dirs::data_dir()
                .ok_or("data directory unavailable")?
                .join("applications")
                .join(ENTRY_NAME),
            icon: persistence::data_path("launcher-icon.png")?,
        })
    }

    fn status(&self) -> Result<LauncherStatus, String> {
        let installed = match std::fs::read_to_string(&self.entry) {
            Ok(content) => content.lines().any(|line| line == OWNERSHIP_MARKER),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.to_string()),
        };
        let preferences: Preferences = persistence::load(&self.preferences)?;
        Ok(LauncherStatus {
            installed,
            dismissed: preferences.dismissed,
        })
    }

    fn dismiss(&self) -> Result<LauncherStatus, String> {
        persistence::mutate(&self.preferences, |preferences: &mut Preferences| {
            preferences.dismissed = true;
            Ok(())
        })?;
        self.status()
    }

    fn install(&self, executable: &Path) -> Result<LauncherStatus, String> {
        let executable = desktop::resolve_launcher(executable, None)?;
        let executable = executable.to_str().ok_or("launcher path must be UTF-8")?;
        if executable.contains('=') {
            return Err(
                "Move AppBento to a path without an equals sign before adding it to the launcher."
                    .into(),
            );
        }
        let icon = self.icon.to_str().ok_or("icon path must be UTF-8")?;
        if icon.chars().any(char::is_control) {
            return Err("icon path cannot contain control characters".into());
        }
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=AppBento\nComment=Manage your AppImages\nExec=\"{}\"\nIcon={}\nTerminal=false\nCategories=Utility;\n{OWNERSHIP_MARKER}\n",
            desktop::escape_exec(executable),
            icon.replace('\\', "\\\\"),
        );
        // Serialize installation with the dismissal preference, and preserve foreign entries.
        persistence::mutate(&self.preferences, |preferences: &mut Preferences| {
            match std::fs::read_to_string(&self.entry) {
                Ok(existing) if !existing.lines().any(|line| line == OWNERSHIP_MARKER) => {
                    return Err(
                        "An existing AppBento launcher entry is not managed by AppBento.".into(),
                    );
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            persistence::write_atomic(&self.icon, ICON).map_err(|error| error.message)?;
            persistence::write_atomic(&self.entry, content.as_bytes())
                .map_err(|error| error.message)?;
            preferences.dismissed = true;
            Ok(())
        })?;
        self.status()
    }
}

#[tauri::command]
pub async fn self_launcher_status() -> Result<LauncherStatus, String> {
    tokio::task::spawn_blocking(|| Launcher::current()?.status())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn self_launcher_dismiss() -> Result<LauncherStatus, String> {
    tokio::task::spawn_blocking(|| Launcher::current()?.dismiss())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn self_launcher_install() -> Result<LauncherStatus, String> {
    tokio::task::spawn_blocking(|| {
        let executable = desktop::resolve_launcher(
            &std::env::current_exe().map_err(|error| error.to_string())?,
            std::env::var_os("APPIMAGE").as_deref().map(Path::new),
        )?;
        Launcher::current()?.install(&executable)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
#[path = "self_launcher_tests.rs"]
mod tests;
