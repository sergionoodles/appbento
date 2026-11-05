//! Bubblewrap installation and namespace capability checks.
use serde::Serialize;
use std::fs;
use std::process::Command;

const BWRAP_CANDIDATES: &[&str] = &["/usr/bin/bwrap", "/usr/local/bin/bwrap", "/bin/bwrap"];
const PROXY_CANDIDATES: &[&str] = &[
    "/usr/bin/xdg-dbus-proxy",
    "/usr/local/bin/xdg-dbus-proxy",
    "/bin/xdg-dbus-proxy",
];

#[derive(Debug, Serialize)]
pub struct SandboxStatus {
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
    pub desktop_integration_available: bool,
    pub desktop_integration_error: Option<String>,
}

impl SandboxStatus {
    fn bubblewrap(available: bool, version: Option<String>, error: Option<String>) -> Self {
        Self {
            available,
            version,
            error,
            desktop_integration_available: false,
            desktop_integration_error: None,
        }
    }
}

fn find_executable(candidates: &'static [&'static str]) -> Option<&'static str> {
    use std::os::unix::fs::PermissionsExt;
    candidates.iter().copied().find(|candidate| {
        fs::metadata(candidate)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}

pub(super) fn bwrap_program() -> &'static str {
    find_executable(BWRAP_CANDIDATES).unwrap_or(BWRAP_CANDIDATES[0])
}

pub(super) fn xdg_dbus_proxy_program() -> Option<&'static str> {
    find_executable(PROXY_CANDIDATES)
}

/// Whether filtered session bus access can be provided, on top of Bubblewrap itself.
fn desktop_integration_error() -> Option<String> {
    let Some(program) = xdg_dbus_proxy_program() else {
        return Some("xdg-dbus-proxy is not installed. Install it (on Arch Linux: sudo pacman -S xdg-dbus-proxy) to use Desktop integration".into());
    };
    match Command::new(program).env_clear().arg("--version").output() {
        Ok(output) if output.status.success() => session_bus_error(),
        Ok(output) => Some(format!(
            "xdg-dbus-proxy does not work: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => Some(format!("xdg-dbus-proxy cannot be executed: {error}")),
    }
}

fn session_bus_error() -> Option<String> {
    let Some(directory) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return Some("XDG_RUNTIME_DIR is not set, so there is no session bus to share".into());
    };
    super::dbus::resolve_bus_socket(
        std::env::var("DBUS_SESSION_BUS_ADDRESS").ok().as_deref(),
        std::path::Path::new(&directory),
    )
    .err()
}

pub fn status() -> SandboxStatus {
    let mut status = bubblewrap_status();
    let error = desktop_integration_error();
    status.desktop_integration_available = status.available && error.is_none();
    status.desktop_integration_error = error;
    status
}

fn bubblewrap_status() -> SandboxStatus {
    let version = match Command::new(bwrap_program())
        .env_clear()
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => {
            return SandboxStatus::bubblewrap(
                false,
                None,
                Some(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
            )
        }
        Err(error) => {
            return SandboxStatus::bubblewrap(
                false,
                None,
                Some(format!(
                    "Bubblewrap is not installed or cannot be executed: {error}"
                )),
            )
        }
    };
    let support = Command::new(bwrap_program())
        .env_clear()
        .arg("--help")
        .output();
    if !support.is_ok_and(|output| {
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("--ro-bind-fd")
    }) {
        return SandboxStatus::bubblewrap(false, Some(version), Some("Bubblewrap must support --ro-bind-fd (version 0.13 or newer). On Arch Linux, upgrade with sudo pacman -Syu bubblewrap".into()));
    }
    let probe = Command::new(bwrap_program())
        .env_clear()
        .args([
            "--unshare-all",
            "--unshare-user",
            "--new-session",
            "--die-with-parent",
            "--ro-bind",
            "/",
            "/",
            "--",
            "/bin/true",
        ])
        .output();
    let error = match probe {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(format!(
            "Bubblewrap is installed but namespaces are unavailable: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => Some(format!("Bubblewrap capability check failed: {error}")),
    };
    SandboxStatus::bubblewrap(error.is_none(), Some(version), error)
}
