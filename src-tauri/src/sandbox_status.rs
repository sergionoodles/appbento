//! Bubblewrap installation and namespace capability checks.
use serde::Serialize;
use std::fs;
use std::process::Command;

const BWRAP_CANDIDATES: &[&str] = &["/usr/bin/bwrap", "/usr/local/bin/bwrap", "/bin/bwrap"];

#[derive(Debug, Serialize)]
pub struct SandboxStatus {
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

pub(super) fn bwrap_program() -> &'static str {
    use std::os::unix::fs::PermissionsExt;
    BWRAP_CANDIDATES
        .iter()
        .copied()
        .find(|candidate| {
            fs::metadata(candidate).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .unwrap_or(BWRAP_CANDIDATES[0])
}

pub fn status() -> SandboxStatus {
    let version = match Command::new(bwrap_program())
        .env_clear()
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        Ok(output) => {
            return SandboxStatus {
                available: false,
                version: None,
                error: Some(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
            }
        }
        Err(error) => {
            return SandboxStatus {
                available: false,
                version: None,
                error: Some(format!(
                    "Bubblewrap is not installed or cannot be executed: {error}"
                )),
            }
        }
    };
    let support = Command::new(bwrap_program())
        .env_clear()
        .arg("--help")
        .output();
    if !support.is_ok_and(|output| {
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("--ro-bind-fd")
    }) {
        return SandboxStatus { available: false, version: Some(version), error: Some("Bubblewrap must support --ro-bind-fd (version 0.13 or newer). On Arch Linux, upgrade with sudo pacman -Syu bubblewrap".into()) };
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
    SandboxStatus {
        available: error.is_none(),
        version: Some(version),
        error,
    }
}
