//! Launch policies and an explicit, deny-by-default Bubblewrap filesystem.
use std::fs;
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::process::Command;

#[path = "sandbox_identity.rs"]
mod identity;
#[path = "sandbox_runtime.rs"]
mod runtime;
pub use identity::Identity;
use runtime::add_devices;
pub use runtime::Runtime;
use sha2::{Digest, Sha256};

#[cfg(test)]
use crate::lifecycle::app_file::appimage_digest;
use crate::lifecycle::{digest_file, open_app};
use crate::models::{
    AppKind, DesktopIntegration, InstalledApp, LaunchConfig, LaunchMode, PathAccess,
    PathPermission, SandboxPermissions,
};

#[path = "sandbox_launch.rs"]
mod execution;
pub use crate::launch_monitor::launch;
pub(crate) use execution::launch_observed;

#[path = "sandbox_status.rs"]
mod capability;
use capability::{bwrap_program, xdg_dbus_proxy_program};
pub use capability::{status, SandboxStatus};

#[path = "sandbox_target.rs"]
mod target;

#[path = "sandbox_shell.rs"]
mod shell;
pub use shell::manual_command;

#[path = "sandbox_dbus.rs"]
mod dbus;
pub use dbus::DesktopSession;

#[path = "sandbox_dbus_process.rs"]
mod desktop_process;

#[cfg(test)]
#[path = "sandbox_dbus_cleanup_tests.rs"]
mod dbus_cleanup_tests;

/// Clears desktop-integration directories of launch monitors that were killed, off the UI thread.
pub fn remove_stale_desktop_directories_at_startup() {
    let Some(runtime_directory) = Runtime::current().directory else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("appbento-desktop-cleanup".into())
        .spawn(move || {
            if let Err(error) = desktop_process::remove_stale_directories(&runtime_directory) {
                eprintln!("desktop integration cleanup failed: {error}");
            }
        });
    if let Err(error) = spawned {
        eprintln!("cannot start desktop integration cleanup: {error}");
    }
}

#[path = "launch_options.rs"]
mod launch_options;

#[cfg(test)]
#[path = "sandbox_network_tests.rs"]
mod network_tests;

const SYSTEM_RUNTIME: &[&str] = &["/usr", "/bin", "/sbin", "/lib", "/lib64"];
const SYSTEM_CONFIG: &[&str] = &[
    "/etc/ld.so.cache",
    "/etc/fonts",
    "/etc/localtime",
    // Public CA stores are needed independently of permission to access the network.
    "/etc/ssl/certs",
    "/etc/ssl/cert.pem",
    "/etc/ca-certificates/extracted",
    "/etc/pki/tls/certs",
    "/etc/pki/tls/cert.pem",
    "/etc/pki/ca-trust/extracted",
    // Rebuilding font caches can crash apps that bundle a conflicting FreeType version.
    "/var/cache/fontconfig",
];
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
    "/var/cache/fontconfig",
    "/tmp/.X11-unix",
];

pub fn validate_config(
    config: &LaunchConfig,
    policy_directory: &Path,
) -> Result<LaunchConfig, String> {
    let canonical = validate_restored_config(config, policy_directory)?;
    dbus::validate(&canonical.permissions.desktop_integration)?;
    // Host networking also exposes abstract X11 sockets, including servers started later.
    if config.mode == LaunchMode::Bubblewrap
        && config.permissions.network
        && !config.permissions.display
    {
        return Err("Internet and network requires Display because sharing the host network can expose X11 windows and keyboard input. Enable Display explicitly, or turn off Internet and network".into());
    }
    Ok(canonical)
}

// Backups retain older permission choices for editing, but never retain approval
// when those choices fail the current launch policy.
pub(crate) fn validate_restored_config(
    config: &LaunchConfig,
    policy_directory: &Path,
) -> Result<LaunchConfig, String> {
    launch_options::validate(config)?;
    dbus::validate_for_restore(&config.permissions.desktop_integration)?;
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
        arguments: config.arguments.clone(),
        environment: config.environment.clone(),
        mode: config.mode.clone(),
        permissions: SandboxPermissions {
            paths,
            ..config.permissions.clone()
        },
    })
}

pub fn default_permissions(app: &InstalledApp, policy_directory: &Path) -> SandboxPermissions {
    let mut permissions = SandboxPermissions {
        desktop_integration: DesktopIntegration::recommended(),
        ..SandboxPermissions::default()
    };
    if app.kind == AppKind::Binary {
        if let Some(parent) = Path::new(&app.file_path).parent() {
            if let Ok(path) = validate_grant(parent, policy_directory) {
                permissions.paths.push(PathPermission {
                    path: path.to_string_lossy().into_owned(),
                    access: PathAccess::ReadOnly,
                });
            }
        }
    }
    permissions
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
    pub identity: Option<Identity>,
    pub environment: std::collections::BTreeMap<String, String>,
    pub program: String,
    pub arguments: Vec<String>,
    pub clear_environment: bool,
    pub application_destination: Option<String>,
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
    launch_options::validate(config)?;
    let mut file = open_app(Path::new(&app.file_path), app.kind)?;
    if config.mode == LaunchMode::Direct {
        return Ok(LaunchPlan {
            identity: None,
            program: app.file_path.clone(),
            arguments: config
                .arguments
                .iter()
                .chain(uri_arguments)
                .cloned()
                .collect(),
            environment: config.environment.clone(),
            clear_environment: false,
            application_destination: None,
        });
    }
    runtime.identity.validate()?;
    let config = validate_config(config, policy_directory)?;
    let target = target::prepare(app, &mut file, &config.permissions)?;
    // The detached monitor owns this sandbox, so closing the UI leaves it alive.
    let mut arguments = vec![
        "--unshare-user",
        "--unshare-pid",
        "--unshare-uts",
        "--unshare-cgroup-try",
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
    arguments.extend([
        "--uid".into(),
        runtime.identity.uid.to_string(),
        "--gid".into(),
        runtime.identity.gid.to_string(),
        "--setenv".into(),
        "USER".into(),
        "app".into(),
        "--setenv".into(),
        "LOGNAME".into(),
        "app".into(),
    ]);
    for path in SYSTEM_RUNTIME.iter().chain(SYSTEM_CONFIG) {
        if Path::new(path).exists() {
            bind(&mut arguments, "--ro-bind", path, path);
        }
    }
    // X11 MIT-SHM uses the host IPC namespace; keep other launches isolated.
    if !config.permissions.display || runtime.available_x11_display().is_none() {
        arguments.push("--unshare-ipc".into());
    }
    if !config.permissions.network {
        arguments.push("--unshare-net".into());
    }
    if config.permissions.network {
        for path in ["/etc/nsswitch.conf", "/etc/hosts"] {
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
    // Create these after the persistent home mount so they remain visible to the app.
    for path in [
        "/home/app/.config",
        "/home/app/.local/share",
        "/home/app/.cache",
    ] {
        arguments.extend(["--dir".into(), path.into()]);
    }
    add_devices(&mut arguments, &config.permissions, runtime)?;
    dbus::add_app_arguments(
        &mut arguments,
        &config.permissions,
        runtime.desktop_session.as_ref(),
    )?;
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
        &target.destination,
    );
    for (name, value) in &config.environment {
        arguments.extend(["--setenv".into(), name.clone(), value.clone()]);
    }
    arguments.extend(
        ["--chdir", "/home/app", "--"]
            .into_iter()
            .map(str::to_owned),
    );
    arguments.extend(target.command);
    arguments.extend_from_slice(&config.arguments);
    arguments.extend_from_slice(uri_arguments);
    Ok(LaunchPlan {
        identity: Some(runtime.identity),
        program: bwrap_program().into(),
        arguments,
        clear_environment: true,
        application_destination: Some(target.destination),
        environment: Default::default(),
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

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sandbox_lifetime_tests.rs"]
mod lifetime_tests;

#[cfg(test)]
#[path = "sandbox_dbus_runtime_tests.rs"]
mod dbus_runtime_tests;

#[cfg(test)]
#[path = "launch_detachment_tests.rs"]
mod detachment_tests;

#[cfg(test)]
#[path = "sandbox_gpu_tests.rs"]
mod gpu_tests;

#[cfg(test)]
#[path = "sandbox_certificates_tests.rs"]
mod certificates_tests;

#[cfg(test)]
#[path = "sandbox_binary_tests.rs"]
mod binary_tests;

#[cfg(test)]
#[path = "sandbox_binary_runtime_tests.rs"]
mod binary_runtime_tests;

#[cfg(test)]
#[path = "sandbox_identity_tests.rs"]
mod identity_tests;
