//! Pure planning for filtered session bus access and xdg-desktop-portal integration.
//!
//! The app never sees the host bus. A filtering `xdg-dbus-proxy` exposes only the names the user
//! allowed, and a Flatpak-style `/.flatpak-info` lets the portals identify the sandbox by app ID.
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::bind;
use crate::models::{DbusAccess, DesktopIntegration, SandboxPermissions};

/// Resolved at launch to the descriptor holding the Flatpak-style keyfile for the app.
pub(super) const APP_INFO_PLACEHOLDER: &str = "@desktop-app-info-fd";
/// Resolved at launch to the descriptor Bubblewrap writes `bwrapinfo.json` into.
pub(super) const BWRAP_INFO_PLACEHOLDER: &str = "@desktop-bwrap-info-fd";

const APP_ID_PREFIX: &str = "io.appbento.App.a";
const APP_ID_HASH_CHARACTERS: usize = 16;
pub(super) const INSTANCE_ID_PREFIX: &str = "appbento-";
/// Flatpak-compatible instance directories, relative to `XDG_RUNTIME_DIR`.
pub(super) const INSTANCE_ROOT: &str = ".flatpak";
/// Proxy socket directories, relative to `XDG_RUNTIME_DIR`.
pub(super) const PROXY_ROOT: &str = "appbento/dbus";
const MAX_CUSTOM_RULES: usize = 32;
const MAX_BUS_NAME_LENGTH: usize = 255;
const BUS_DRIVER_NAME: &str = "org.freedesktop.DBus";
const SANDBOX_BUS_PATH: &str = "/run/user/app/bus";
const MISSING_SESSION: &str = "Desktop integration needs a session bus. Turn off Desktop integration in this app's permissions";

/// Everything the planner needs to wire an app to a proxied session bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSession {
    pub app_id: String,
    pub instance_id: String,
    pub runtime_directory: PathBuf,
    pub bus_socket: PathBuf,
    /// The document portal's per-app view, bound where portal-returned paths point.
    pub document_store: Option<PathBuf>,
}

impl DesktopSession {
    /// Where the portals look up the sandbox (`bwrapinfo.json`, `.ref`).
    pub fn instance_directory(&self) -> PathBuf {
        self.runtime_directory
            .join(INSTANCE_ROOT)
            .join(&self.instance_id)
    }

    pub fn proxy_directory(&self) -> PathBuf {
        self.runtime_directory
            .join(PROXY_ROOT)
            .join(&self.instance_id)
    }

    pub fn proxy_socket(&self) -> PathBuf {
        self.proxy_directory().join("bus")
    }

    /// Keyfile contents mounted at `/.flatpak-info`.
    pub fn keyfile(&self) -> String {
        format!(
            "[Application]\nname={}\n\n[Instance]\ninstance-id={}\n",
            self.app_id, self.instance_id
        )
    }
}

pub fn app_id(library_id: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(library_id.as_bytes()));
    format!("{APP_ID_PREFIX}{}", &digest[..APP_ID_HASH_CHARACTERS])
}

pub fn instance_id(random: u64) -> String {
    format!("{INSTANCE_ID_PREFIX}{random:016x}")
}

pub fn proxy_rules(integration: &DesktopIntegration) -> Vec<String> {
    let fixed = [
        (integration.portals, "--talk=org.freedesktop.portal.*"),
        (
            integration.notifications,
            "--talk=org.freedesktop.Notifications",
        ),
        (integration.tray, "--talk=org.kde.StatusNotifierWatcher"),
        (integration.media_controls, "--own=org.mpris.MediaPlayer2.*"),
        (integration.secrets, "--talk=org.freedesktop.secrets"),
    ];
    fixed
        .into_iter()
        .filter(|(is_enabled, _)| *is_enabled)
        .map(|(_, rule)| rule.to_owned())
        .chain(integration.custom.iter().map(|rule| {
            let verb = match rule.access {
                DbusAccess::Talk => "talk",
                DbusAccess::Own => "own",
            };
            format!("--{verb}={}", rule.name)
        }))
        .collect()
}

pub fn validate(integration: &DesktopIntegration) -> Result<(), String> {
    if integration.custom.len() > MAX_CUSTOM_RULES {
        return Err(format!(
            "Desktop integration allows at most {MAX_CUSTOM_RULES} custom D-Bus names"
        ));
    }
    integration
        .custom
        .iter()
        .try_for_each(|rule| validate_bus_name(&rule.name))
}

fn validate_bus_name(name: &str) -> Result<(), String> {
    let invalid = |reason: &str| Err(format!("Invalid D-Bus name {name:?}: {reason}"));
    if name.starts_with(':') {
        return invalid("unique connection names are not allowed");
    }
    if name.len() > MAX_BUS_NAME_LENGTH {
        return invalid("name is too long");
    }
    let base = name.strip_suffix(".*").unwrap_or(name);
    let elements = base.split('.').collect::<Vec<_>>();
    if elements.len() < 2 {
        return invalid("a name needs at least two dot-separated elements");
    }
    if !elements.iter().all(|element| is_valid_element(element)) {
        return invalid("elements may only use letters, digits, '_' and '-' and cannot start with a digit; '.*' is only allowed at the end");
    }
    let driver = BUS_DRIVER_NAME.split('.').collect::<Vec<_>>();
    let covers_driver = elements == driver
        || (base != name && elements.len() <= driver.len() && driver[..elements.len()] == elements);
    if covers_driver {
        return invalid("the bus itself cannot be granted");
    }
    Ok(())
}

fn is_valid_element(element: &str) -> bool {
    element
        .chars()
        .next()
        .is_some_and(|first| !first.is_ascii_digit())
        && element
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

/// Path of a `unix:path=` socket in a D-Bus address list; abstract sockets are not supported.
pub fn bus_socket_from_address(address: &str) -> Option<PathBuf> {
    address
        .split(';')
        .filter_map(|entry| entry.strip_prefix("unix:"))
        .flat_map(|keys| keys.split(','))
        .find_map(|key| key.strip_prefix("path="))
        .and_then(decode_address_value)
        .filter(|path| path.is_absolute())
}

fn decode_address_value(value: &str) -> Option<PathBuf> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let digits = value.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(digits, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok().map(PathBuf::from)
}

pub fn resolve_bus_socket(
    address: Option<&str>,
    runtime_directory: &Path,
) -> Result<PathBuf, String> {
    let socket = match address.filter(|address| !address.is_empty()) {
        Some(address) => bus_socket_from_address(address).ok_or_else(|| {
            format!("Desktop integration needs a session bus on a filesystem socket, but DBUS_SESSION_BUS_ADDRESS is {address:?}. Turn off Desktop integration in this app's permissions")
        })?,
        None => runtime_directory.join("bus"),
    };
    if !fs::metadata(&socket).is_ok_and(|metadata| metadata.file_type().is_socket()) {
        return Err(format!(
            "No session bus found at {}. Turn off Desktop integration in this app's permissions",
            socket.display()
        ));
    }
    Ok(socket)
}

/// Mounts and environment that connect the sandbox to the proxied bus and the portals.
pub(super) fn add_app_arguments(
    arguments: &mut Vec<String>,
    permissions: &SandboxPermissions,
    session: Option<&DesktopSession>,
) -> Result<(), String> {
    if !permissions.desktop_integration.is_enabled() {
        return Ok(());
    }
    let session = session.ok_or(MISSING_SESSION)?;
    arguments.extend(
        [
            "--info-fd",
            BWRAP_INFO_PLACEHOLDER,
            "--ro-bind-data",
            APP_INFO_PLACEHOLDER,
            "/.flatpak-info",
        ]
        .map(str::to_owned),
    );
    bind(
        arguments,
        "--bind",
        utf8(&session.proxy_socket())?,
        SANDBOX_BUS_PATH,
    );
    arguments.extend(
        [
            "--setenv",
            "DBUS_SESSION_BUS_ADDRESS",
            &format!("unix:path={SANDBOX_BUS_PATH}"),
        ]
        .map(str::to_owned),
    );
    if let Some(store) = &session.document_store {
        bind(
            arguments,
            "--bind",
            utf8(store)?,
            utf8(&session.runtime_directory.join("doc"))?,
        );
    }
    Ok(())
}

/// Bubblewrap arguments that run `xdg-dbus-proxy` in a minimal sandbox of its own, so the
/// portals find `/.flatpak-info` under `/proc/<proxy pid>/root`.
pub(super) fn proxy_arguments(
    session: &DesktopSession,
    proxy_program: &str,
    rules: &[String],
    info_descriptor: i32,
    sync_descriptor: i32,
) -> Result<Vec<String>, String> {
    let proxy_directory = session.proxy_directory();
    let proxy_directory = utf8(&proxy_directory)?;
    let host_bus = utf8(&session.bus_socket)?;
    let mut arguments = [
        "--unshare-user",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-net",
        "--unshare-uts",
        "--die-with-parent",
        "--cap-drop",
        "ALL",
        "--tmpfs",
        "/",
    ]
    .map(str::to_owned)
    .to_vec();
    add_system_mounts(&mut arguments, Path::new("/"));
    arguments.extend(["--dev", "/dev", "--proc", "/proc"].map(str::to_owned));
    bind(&mut arguments, "--bind", proxy_directory, proxy_directory);
    bind(&mut arguments, "--bind", host_bus, host_bus);
    arguments.extend([
        "--ro-bind-data".into(),
        info_descriptor.to_string(),
        "/.flatpak-info".into(),
        "--".into(),
        proxy_program.into(),
        format!("unix:path={host_bus}"),
        utf8(&session.proxy_socket())?.into(),
        "--filter".into(),
        format!("--fd={sync_descriptor}"),
    ]);
    arguments.extend_from_slice(rules);
    Ok(arguments)
}

/// Distros differ on whether `/bin`, `/lib` and `/lib64` are symlinks into `/usr`.
fn add_system_mounts(arguments: &mut Vec<String>, root: &Path) {
    bind(arguments, "--ro-bind", "/usr", "/usr");
    for name in ["bin", "sbin", "lib", "lib64"] {
        let path = root.join(name);
        let target = format!("/{name}");
        match fs::read_link(&path) {
            Ok(link) => arguments.extend([
                "--symlink".into(),
                link.to_string_lossy().into_owned(),
                target,
            ]),
            Err(_) if path.is_dir() => bind(arguments, "--ro-bind", &target, &target),
            Err(_) => {}
        }
    }
}

fn utf8(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| "desktop integration paths must be UTF-8".into())
}

#[cfg(test)]
#[path = "sandbox_dbus_tests.rs"]
mod tests;
