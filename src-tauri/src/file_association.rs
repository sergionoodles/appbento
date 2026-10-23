//! Offer AppBento in the desktop "Open With" menu for AppImage files.
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{lifecycle::desktop, persistence, self_launcher};

pub(crate) const OPEN_ARGUMENT: &str = "--open";
const ENTRY_NAME: &str = "appbento-open-appimage.desktop";
const OWNERSHIP_MARKER: &str = "X-AppBento-Open-With=true";
// Type 2 images use the registered type; type 1 images are ISO 9660 based.
const MIME_TYPES: &str = "application/vnd.appimage;application/x-iso9660-appimage;";
const DESKTOP_DATABASE_TOOL: &str = "update-desktop-database";

/// The AppImage the desktop asked AppBento to open, consumed once by the window.
#[derive(Default)]
pub struct OpenRequest(Mutex<Option<String>>);

impl OpenRequest {
    pub(crate) fn new(path: Option<String>) -> Self {
        Self(Mutex::new(path))
    }

    fn take(&self) -> Result<Option<String>, String> {
        Ok(self.0.lock().map_err(|error| error.to_string())?.take())
    }
}

#[derive(Debug, PartialEq)]
enum Registration {
    Unchanged,
    Written,
}

struct Association {
    entry: PathBuf,
    icon: PathBuf,
}

impl Association {
    fn current() -> Result<Self, String> {
        Ok(Self {
            entry: dirs::data_dir()
                .ok_or("data directory unavailable")?
                .join("applications")
                .join(ENTRY_NAME),
            icon: self_launcher::icon_path()?,
        })
    }

    fn ensure(&self, executable: &Path) -> Result<Registration, String> {
        let content = entry_content(executable, &self.icon)?;
        let existing = match std::fs::read_to_string(&self.entry) {
            Ok(existing) => Some(existing),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        if existing
            .as_deref()
            .is_some_and(|existing| !existing.lines().any(|line| line == OWNERSHIP_MARKER))
        {
            return Err("An existing AppBento Open With entry is not managed by AppBento.".into());
        }

        if !std::fs::read(&self.icon).is_ok_and(|icon| icon == self_launcher::ICON) {
            persistence::write_atomic(&self.icon, self_launcher::ICON)
                .map_err(|error| error.message)?;
        }

        if existing.as_deref() == Some(content.as_str()) {
            return Ok(Registration::Unchanged);
        }
        persistence::write_atomic(&self.entry, content.as_bytes())
            .map_err(|error| error.message)?;
        Ok(Registration::Written)
    }
}

fn entry_content(executable: &Path, icon: &Path) -> Result<String, String> {
    let executable = desktop::resolve_launcher(executable, None)?;
    let executable = executable.to_str().ok_or("launcher path must be UTF-8")?;
    if executable.contains('=') {
        return Err("AppBento must be in a path without an equals sign to open AppImages.".into());
    }
    let icon = icon.to_str().ok_or("icon path must be UTF-8")?;
    if icon.chars().any(char::is_control) {
        return Err("icon path cannot contain control characters".into());
    }
    // NoDisplay keeps this out of app menus while still offering it for AppImage files.
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=AppBento\nComment=Add this AppImage to AppBento\nExec=\"{}\" {OPEN_ARGUMENT} %f\nIcon={}\nTerminal=false\nNoDisplay=true\nMimeType={MIME_TYPES}\n{OWNERSHIP_MARKER}\n",
        desktop::escape_exec(executable),
        icon.replace('\\', "\\\\"),
    ))
}

/// Returns the file passed by an "Open With" launch, if this is one.
pub(crate) fn requested_path(arguments: &[String]) -> Option<String> {
    match arguments {
        [flag, path, ..] if flag == OPEN_ARGUMENT && !path.is_empty() => Some(path.clone()),
        _ => None,
    }
}

fn register(appimage: &Path) -> Result<(), String> {
    let association = Association::current()?;
    if association.ensure(appimage)? == Registration::Unchanged {
        return Ok(());
    }
    let directory = association
        .entry
        .parent()
        .ok_or("applications directory unavailable")?;
    refresh_desktop_database(directory)
}

fn refresh_desktop_database(directory: &Path) -> Result<(), String> {
    match std::process::Command::new(DESKTOP_DATABASE_TOOL)
        .arg(directory)
        .status()
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{DESKTOP_DATABASE_TOOL} exited with {status}")),
        // Desktops without the tool watch the applications directory themselves.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// Verifies the "Open With" entry in the background so startup is not delayed.
pub(crate) fn register_at_startup() {
    // Only the AppImage has a stable path; development binaries would leave stale entries.
    let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("appbento-open-with".into())
        .spawn(move || {
            if let Err(error) = register(&appimage) {
                eprintln!("AppImage Open With registration failed: {error}");
            }
        });
    if let Err(error) = spawned {
        eprintln!("AppImage Open With registration failed: {error}");
    }
}

#[tauri::command]
pub fn open_request_take(request: tauri::State<'_, OpenRequest>) -> Result<Option<String>, String> {
    request.take()
}

#[cfg(test)]
#[path = "file_association_tests.rs"]
mod tests;
