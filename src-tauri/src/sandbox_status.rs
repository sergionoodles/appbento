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
const ERRNO_INVALID_ARGUMENT: i32 = 22;
const ERRNO_NOT_IMPLEMENTED: i32 = 38;

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
    let mut probe_command = Command::new(bwrap_program());
    probe_command.env_clear().args([
        "--unshare-all",
        "--unshare-user",
        "--new-session",
        "--die-with-parent",
        "--ro-bind",
        "/",
        "/",
        "--",
        "/bin/true",
    ]);
    super::execution::inherit_only_descriptors(&mut probe_command, Vec::new());
    let probe = probe_command.output();
    let error = match probe {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(format!(
            "Bubblewrap is installed but namespaces are unavailable: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => Some(describe_probe_spawn_error(&error)),
    };
    SandboxStatus::bubblewrap(error.is_none(), Some(version), error)
}

fn describe_probe_spawn_error(error: &std::io::Error) -> String {
    let hint = match error.raw_os_error() {
        Some(ERRNO_INVALID_ARGUMENT | ERRNO_NOT_IMPLEMENTED) => ". Safe descriptor isolation requires Linux 5.11 or newer and a syscall policy allowing close_range",
        _ => "",
    };
    format!("Bubblewrap capability check failed: {error}{hint}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrelated_probe_errors_do_not_blame_the_kernel() {
        for code in [1, 2, 8] {
            let error = std::io::Error::from_raw_os_error(code);
            let message = describe_probe_spawn_error(&error);
            assert!(message.contains(&error.to_string()));
            assert!(!message.contains("Linux 5.11"));
        }
    }

    #[test]
    fn unsupported_probe_errors_explain_kernel_and_syscall_policy_requirements() {
        for code in [ERRNO_INVALID_ARGUMENT, ERRNO_NOT_IMPLEMENTED] {
            let message = describe_probe_spawn_error(&std::io::Error::from_raw_os_error(code));
            assert!(message.contains("Linux 5.11"));
            assert!(message.contains("syscall policy"));
        }
    }
}
