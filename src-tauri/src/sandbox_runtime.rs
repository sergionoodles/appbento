use super::bind;
use crate::models::SandboxPermissions;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Runtime {
    pub directory: Option<PathBuf>,
    pub wayland_display: Option<String>,
    pub x11_display: Option<String>,
    pub xauthority: Option<PathBuf>,
    pub pulse_cookie: Option<PathBuf>,
}

impl Runtime {
    pub fn current() -> Self {
        Self {
            directory: std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
            wayland_display: std::env::var("WAYLAND_DISPLAY").ok(),
            x11_display: std::env::var("DISPLAY").ok(),
            xauthority: std::env::var_os("XAUTHORITY")
                .map(PathBuf::from)
                .or_else(|| dirs::home_dir().map(|home| home.join(".Xauthority"))),
            pulse_cookie: dirs::home_dir().map(|home| home.join(".config/pulse/cookie")),
        }
    }
}

pub(super) fn add_devices(
    arguments: &mut Vec<String>,
    permissions: &SandboxPermissions,
    runtime: &Runtime,
) -> Result<(), String> {
    if permissions.display {
        if let (Some(directory), Some(display)) = (&runtime.directory, &runtime.wayland_display) {
            if !display.is_empty()
                && display.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
            {
                let socket = directory.join(display);
                if socket.exists() {
                    bind(
                        arguments,
                        "--ro-bind",
                        socket.to_str().ok_or("invalid display socket")?,
                        &format!("/run/user/app/{display}"),
                    );
                    arguments.extend([
                        "--setenv".into(),
                        "WAYLAND_DISPLAY".into(),
                        display.clone(),
                    ]);
                }
            }
        }
        if let Some(display) = &runtime.x11_display {
            let number = display
                .strip_prefix(':')
                .and_then(|value| value.split('.').next())
                .filter(|number| {
                    !number.is_empty() && number.chars().all(|character| character.is_ascii_digit())
                });
            if let Some(number) = number {
                let socket = format!("/tmp/.X11-unix/X{number}");
                if Path::new(&socket).exists() {
                    bind(arguments, "--ro-bind", &socket, &socket);
                    arguments.extend(["--setenv".into(), "DISPLAY".into(), format!(":{number}")]);
                    bind_credential(
                        arguments,
                        runtime.xauthority.as_deref(),
                        "/run/user/app/Xauthority",
                        "XAUTHORITY",
                    )?;
                }
            }
        }
    }
    if permissions.audio {
        if let Some(directory) = &runtime.directory {
            let socket = directory.join("pulse/native");
            if socket.exists() {
                bind(
                    arguments,
                    "--ro-bind",
                    socket.to_str().ok_or("invalid audio socket")?,
                    "/run/user/app/pulse/native",
                );
                arguments.extend([
                    "--setenv".into(),
                    "PULSE_SERVER".into(),
                    "unix:/run/user/app/pulse/native".into(),
                ]);
                bind_credential(
                    arguments,
                    runtime.pulse_cookie.as_deref(),
                    "/run/user/app/pulse-cookie",
                    "PULSE_COOKIE",
                )?;
            }
        }
    }
    if permissions.gpu && Path::new("/dev/dri").exists() {
        bind(arguments, "--dev-bind", "/dev/dri", "/dev/dri");
        // Drivers resolve DRM device numbers through sysfs to identify the GPU.
        for path in ["/sys/dev/char", "/sys/devices"] {
            if Path::new(path).exists() {
                bind(arguments, "--ro-bind", path, path);
            }
        }
    }
    Ok(())
}

fn bind_credential(
    arguments: &mut Vec<String>,
    source: Option<&Path>,
    target: &str,
    variable: &str,
) -> Result<(), String> {
    let Some(source) = source.filter(|path| path.is_absolute() && path.is_file()) else {
        return Ok(());
    };
    bind(
        arguments,
        "--ro-bind",
        source.to_str().ok_or("credential path must be UTF-8")?,
        target,
    );
    arguments.extend(["--setenv".into(), variable.into(), target.into()]);
    Ok(())
}
