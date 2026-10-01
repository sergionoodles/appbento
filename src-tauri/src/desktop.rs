//! Safe desktop-entry identities and serialization.
use crate::models::DesktopHints;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn desktop_id(path: &Path) -> String {
    format!(
        "appbento-{:x}.desktop",
        Sha256::digest(path.as_os_str().as_encoded_bytes())
    )
}

#[cfg(test)]
pub fn desktop_content(path: &Path, name: &str) -> Result<String, String> {
    desktop_content_with_hints(
        path,
        &path.to_string_lossy(),
        name,
        &DesktopHints::default(),
    )
}

pub(super) fn desktop_content_with_hints(
    path: &Path,
    app_id: &str,
    name: &str,
    hints: &DesktopHints,
) -> Result<String, String> {
    let executable = resolve_launcher(
        &std::env::current_exe().map_err(|error| error.to_string())?,
        std::env::var_os("APPIMAGE").as_deref().map(Path::new),
    )?;
    let old = legacy_desktop_content(path, name, hints);
    let executable = escape_exec(&executable.to_string_lossy());
    let id = escape_exec(app_id);
    Ok(old
        .lines()
        .map(|line| {
            if line.starts_with("Exec=") {
                format!("Exec=\"{executable}\" --launch \"{id}\" %U")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n")
}

fn escape_exec(value: &str) -> String {
    value
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%")
}

pub(super) fn legacy_desktop_content(path: &Path, name: &str, hints: &DesktopHints) -> String {
    let display_name = name
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    let escaped_path = path
        .to_string_lossy()
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%");
    let terminal = hints.terminal;
    let categories = hints.categories.as_deref().unwrap_or("Utility;");
    let mime_type = hints
        .mime_type
        .as_ref()
        .map(|value| format!("MimeType={value}\n"))
        .unwrap_or_default();
    format!("[Desktop Entry]\nType=Application\nName={display_name}\nExec=\"{escaped_path}\" %U\nTerminal={terminal}\nCategories={categories}\n{mime_type}X-AppBento-Managed=true\n")
}

pub(super) fn migrate(path: &Path, app: &crate::models::InstalledApp) -> Result<(), String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    let app_path = Path::new(&app.file_path);
    let current =
        desktop_content_with_hints(app_path, &app.id, &app.name, &DesktopHints::default())?;
    let legacy = legacy_desktop_content(app_path, &app.name, &DesktopHints::default());
    let expected_exec = current
        .lines()
        .find(|line| line.starts_with("Exec="))
        .ok_or("missing Exec")?;
    let legacy_exec = legacy
        .lines()
        .find(|line| line.starts_with("Exec="))
        .ok_or("missing Exec")?;
    if !content
        .lines()
        .any(|line| line == "X-AppBento-Managed=true")
    {
        return Err("desktop entry ownership cannot be verified".into());
    }
    if content.lines().any(|line| line == expected_exec) {
        return Ok(());
    }
    if !content.lines().any(|line| line == legacy_exec) {
        return Err("desktop entry ownership cannot be verified".into());
    }
    let migrated = content
        .lines()
        .map(|line| {
            if line.starts_with("Exec=") {
                expected_exec
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    crate::persistence::write_atomic(path, migrated.as_bytes()).map_err(|error| error.message)
}

pub(super) fn resolve_launcher(
    current_executable: &Path,
    appimage: Option<&Path>,
) -> Result<PathBuf, String> {
    let launcher = appimage.unwrap_or(current_executable);
    if !launcher.is_absolute()
        || launcher.to_str().is_none()
        || launcher.to_string_lossy().chars().any(char::is_control)
        || !launcher.is_file()
    {
        return Err("AppBento launcher must be an existing absolute executable path without control characters".into());
    }
    Ok(launcher.to_owned())
}
