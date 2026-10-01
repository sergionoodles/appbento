//! Safe desktop-entry identities and serialization.
use crate::models::DesktopHints;
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn desktop_id(path: &Path) -> String {
    format!(
        "appbento-{:x}.desktop",
        Sha256::digest(path.as_os_str().as_encoded_bytes())
    )
}

pub fn desktop_content(path: &Path, name: &str) -> String {
    desktop_content_with_hints(path, name, &DesktopHints::default())
}

pub(super) fn desktop_content_with_hints(path: &Path, name: &str, hints: &DesktopHints) -> String {
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
