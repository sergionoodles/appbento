//! Launch policies and an explicit, deny-by-default Bubblewrap filesystem.
use std::fs;
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde::Serialize;

#[path = "sandbox_runtime.rs"]
mod runtime;
use runtime::add_devices;
pub use runtime::Runtime;
use sha2::{Digest, Sha256};

use crate::lifecycle::open_appimage;
use crate::models::{InstalledApp, LaunchConfig, LaunchMode, PathAccess, SandboxPermissions};

#[path = "sandbox_launch.rs"]
mod execution;
pub use execution::{launch, launch_with_wait};

const SYSTEM_RUNTIME: &[&str] = &["/usr", "/bin", "/sbin", "/lib", "/lib64"];
const BWRAP_CANDIDATES: &[&str] = &["/usr/bin/bwrap", "/usr/local/bin/bwrap", "/bin/bwrap"];
const SYSTEM_CONFIG: &[&str] = &["/etc/ld.so.cache", "/etc/fonts", "/etc/localtime"];
const RESERVED: &[&str] = &[
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/etc",
    "/dev",
    "/proc",
    "/sys",
    "/run",
    "/app",
    "/home/app",
    "/tmp/.X11-unix",
];

#[derive(Debug, Serialize)]
pub struct SandboxStatus {
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

fn bwrap_program() -> &'static str {
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

pub fn validate_config(
    config: &LaunchConfig,
    policy_directory: &Path,
) -> Result<LaunchConfig, String> {
    let paths = config
        .permissions
        .paths
        .iter()
        .map(|grant| {
            let path = validate_grant(Path::new(&grant.path), policy_directory)?;
            Ok(crate::models::PathPermission {
                path: path.to_str().ok_or("folder path must be UTF-8")?.into(),
                access: grant.access.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let unique_paths = paths
        .iter()
        .map(|grant| &grant.path)
        .collect::<std::collections::HashSet<_>>();
    if unique_paths.len() != paths.len() {
        return Err("folder grants must not contain duplicate paths".into());
    }
    Ok(LaunchConfig {
        mode: config.mode.clone(),
        permissions: SandboxPermissions {
            paths,
            ..config.permissions.clone()
        },
    })
}

fn validate_grant(path: &Path, policy_directory: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || path.to_string_lossy().chars().any(char::is_control)
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(
            "folder grants must be absolute paths without control characters or traversal".into(),
        );
    }
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("cannot access folder {}: {error}", path.display()))?;
    if !canonical.is_dir() {
        return Err("folder grants must refer to existing directories".into());
    }
    let policy = fs::canonicalize(policy_directory).unwrap_or_else(|_| policy_directory.to_owned());
    if RESERVED
        .iter()
        .map(Path::new)
        .chain(std::iter::once(policy.as_path()))
        .any(|reserved| canonical.starts_with(reserved) || reserved.starts_with(&canonical))
    {
        return Err("folder grant would expose protected runtime or AppBento policy data".into());
    }
    Ok(canonical)
}

#[derive(Debug)]
pub struct LaunchPlan {
    pub program: String,
    pub arguments: Vec<String>,
    pub clear_environment: bool,
}

pub fn plan(
    app: &InstalledApp,
    uri_arguments: &[String],
    policy_directory: &Path,
    runtime: &Runtime,
) -> Result<LaunchPlan, String> {
    let config = app
        .launch_config
        .as_ref()
        .ok_or("Review and confirm launch permissions before the first launch")?;
    let mut file = open_appimage(Path::new(&app.file_path))?;
    if config.mode == LaunchMode::Direct {
        return Ok(LaunchPlan {
            program: app.file_path.clone(),
            arguments: uri_arguments.to_vec(),
            clear_environment: false,
        });
    }
    let config = validate_config(config, policy_directory)?;
    let mut header = [0; 11];
    file.rewind().map_err(|error| error.to_string())?;
    file.read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    if header[10] != 2 {
        return Err(
            "Bubblewrap supports Type 2 AppImages only; Type 1 is not supported by this sandbox launcher".into(),
        );
    }
    let mut arguments = vec![
        "--unshare-all",
        "--unshare-user",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--clearenv",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--dir",
        "/app",
        "--dir",
        "/home/app",
        "--perms",
        "0700",
        "--dir",
        "/run/user/app",
        "--setenv",
        "HOME",
        "/home/app",
        "--setenv",
        "PATH",
        "/usr/bin:/bin",
        "--setenv",
        "TMPDIR",
        "/tmp",
        "--setenv",
        "XDG_RUNTIME_DIR",
        "/run/user/app",
        "--setenv",
        "XDG_CONFIG_HOME",
        "/home/app/.config",
        "--setenv",
        "XDG_DATA_HOME",
        "/home/app/.local/share",
        "--setenv",
        "XDG_CACHE_HOME",
        "/home/app/.cache",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    for path in SYSTEM_RUNTIME.iter().chain(SYSTEM_CONFIG) {
        if Path::new(path).exists() {
            bind(&mut arguments, "--ro-bind", path, path);
        }
    }
    if config.permissions.network {
        arguments.push("--share-net".into());
        for path in [
            "/etc/ssl/certs",
            "/etc/pki/tls/certs",
            "/etc/nsswitch.conf",
            "/etc/hosts",
        ] {
            if Path::new(path).exists() {
                bind(&mut arguments, "--ro-bind", path, path);
            }
        }
        if Path::new("/etc/resolv.conf").exists() {
            bind(
                &mut arguments,
                "--ro-bind",
                "/etc/resolv.conf",
                "/etc/resolv.conf",
            );
        }
    }
    if config.permissions.persistent_data {
        let home = private_home_path(policy_directory, &app.id);
        bind(
            &mut arguments,
            "--bind",
            home.to_str().ok_or("private home path must be UTF-8")?,
            "/home/app",
        );
    }
    add_devices(&mut arguments, &config.permissions, runtime)?;
    for grant in &config.permissions.paths {
        bind(
            &mut arguments,
            if grant.access == PathAccess::ReadOnly {
                "--ro-bind"
            } else {
                "--bind"
            },
            &grant.path,
            &grant.path,
        );
    }
    bind(
        &mut arguments,
        "--ro-bind",
        &app.file_path,
        "/app/application.AppImage",
    );
    arguments.extend(
        [
            "--chdir",
            "/home/app",
            "--",
            "/app/application.AppImage",
            "--appimage-extract-and-run",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    arguments.extend_from_slice(uri_arguments);
    Ok(LaunchPlan {
        program: bwrap_program().into(),
        arguments,
        clear_environment: true,
    })
}

fn bind(arguments: &mut Vec<String>, option: &str, source: &str, target: &str) {
    arguments.extend([option, source, target].into_iter().map(str::to_owned));
}

fn private_home_path(policy_directory: &Path, id: &str) -> PathBuf {
    policy_directory
        .join("sandbox-data")
        .join(format!("{:x}", Sha256::digest(id.as_bytes())))
}

pub fn appimage_digest(path: &Path) -> Result<String, String> {
    let mut file = open_appimage(path)?;
    digest_file(&mut file)
}

fn digest_file(file: &mut fs::File) -> Result<String, String> {
    file.rewind().map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
