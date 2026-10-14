//! Safe desktop-entry identities and serialization.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn desktop_id(path: &Path) -> String {
    format!(
        "appbento-{:x}.desktop",
        Sha256::digest(path.as_os_str().as_encoded_bytes())
    )
}

pub(crate) fn entry_path(
    directory: &Path,
    app: &crate::models::InstalledApp,
) -> Result<Option<PathBuf>, String> {
    app.desktop_id
        .as_deref()
        .map(|id| {
            let digest = id
                .strip_prefix("appbento-")
                .and_then(|id| id.strip_suffix(".desktop"))
                .ok_or("desktop entry ownership cannot be verified")?;
            if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("desktop entry ownership cannot be verified".into());
            }
            Ok(directory.join(id))
        })
        .transpose()
}

pub(crate) fn content_for_entry(app: &crate::models::InstalledApp) -> Result<String, String> {
    let content = desktop_content_for_app(Path::new(&app.file_path), &app.id, &app.name)?;
    // The launcher reads the saved policy; the fingerprint records which policy it reflects
    // without embedding environment values in a world-readable desktop entry.
    let config = serde_json::to_vec(&app.launch_config).map_err(|error| error.to_string())?;
    Ok(format!(
        "{content}X-AppBento-Launch-Config={:x}\n",
        Sha256::digest(config)
    ))
}

#[cfg(test)]
pub fn desktop_content(path: &Path, name: &str) -> Result<String, String> {
    desktop_content_for_app(path, &path.to_string_lossy(), name)
}

pub(crate) fn desktop_content_for_app(
    path: &Path,
    app_id: &str,
    name: &str,
) -> Result<String, String> {
    let executable = resolve_launcher(
        &std::env::current_exe().map_err(|error| error.to_string())?,
        std::env::var_os("APPIMAGE").as_deref().map(Path::new),
    )?;
    let old = legacy_desktop_content(path, name);
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

pub(crate) fn escape_exec(value: &str) -> String {
    value
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%")
}

pub(super) fn legacy_desktop_content(path: &Path, name: &str) -> String {
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
    format!("[Desktop Entry]\nType=Application\nName={display_name}\nExec=\"{escaped_path}\" %U\nTerminal=false\nCategories=Utility;\nX-AppBento-Managed=true\n")
}

fn is_previous_launch_exec(line: &str, app_id: &str) -> bool {
    let suffix = format!("\" --launch \"{}\" %U", escape_exec(app_id));
    let Some(launcher) = line
        .strip_prefix("Exec=\"")
        .and_then(|command| command.strip_suffix(&suffix))
    else {
        return false;
    };
    if !launcher.starts_with('/') || launcher == "/" {
        return false;
    }

    // Accept only the quoting emitted by escape_exec, so the launcher stays one argument.
    let escaped_sequences =
        ['\\', '"', '`', '$', '%'].map(|character| escape_exec(&character.to_string()));
    let mut remaining = launcher;
    while !remaining.is_empty() {
        if let Some(rest) = escaped_sequences
            .iter()
            .find_map(|sequence| remaining.strip_prefix(sequence.as_str()))
        {
            remaining = rest;
            continue;
        }
        let Some(character) = remaining.chars().next() else {
            return false;
        };
        if character.is_control() || matches!(character, '\\' | '"' | '`' | '$' | '%') {
            return false;
        }
        remaining = &remaining[character.len_utf8()..];
    }
    true
}

pub(super) fn migrate(path: &Path, app: &crate::models::InstalledApp) -> Result<(), String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    let app_path = Path::new(&app.file_path);
    let current = content_for_entry(app)?;
    let legacy = legacy_desktop_content(app_path, &app.name);
    let expected_exec = current
        .lines()
        .find(|line| line.starts_with("Exec="))
        .ok_or("missing Exec")?;
    let legacy_exec = legacy
        .lines()
        .find(|line| line.starts_with("Exec="))
        .ok_or("missing Exec")?;
    let expected_name = current
        .lines()
        .find(|line| line.starts_with("Name="))
        .ok_or("missing Name")?;
    if !content
        .lines()
        .any(|line| line == "X-AppBento-Managed=true")
    {
        return Err("desktop entry ownership cannot be verified".into());
    }
    if !content
        .lines()
        .any(|line| line == legacy_exec || is_previous_launch_exec(line, &app.id))
    {
        return Err("desktop entry ownership cannot be verified".into());
    }
    // Names change when embedded metadata replaces the filename-derived default.
    let config_line = current
        .lines()
        .find(|line| line.starts_with("X-AppBento-Launch-Config="))
        .ok_or("missing launch config fingerprint")?;
    let migrated = content
        .lines()
        .filter(|line| !line.starts_with("X-AppBento-Launch-Config="))
        .map(|line| {
            if line.starts_with("Exec=") {
                expected_exec
            } else if line.starts_with("Name=") {
                expected_name
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
        + config_line
        + "\n";
    if migrated == content {
        return Ok(());
    }
    crate::persistence::write_atomic(path, migrated.as_bytes()).map_err(|error| error.message)
}

pub(crate) fn resolve_launcher(
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
