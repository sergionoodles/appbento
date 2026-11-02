//! Format-specific targets use the shared sandbox policy and pinned-file mount.
use crate::models::{AppKind, InstalledApp, SandboxPermissions};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

pub(super) struct LaunchTarget {
    pub destination: String,
    pub command: Vec<String>,
}

pub(super) fn prepare(
    app: &InstalledApp,
    file: &mut File,
    permissions: &SandboxPermissions,
) -> Result<LaunchTarget, String> {
    if app.kind == AppKind::AppImage {
        let mut header = [0; 11];
        file.rewind().map_err(|error| error.to_string())?;
        file.read_exact(&mut header)
            .map_err(|error| error.to_string())?;
        if header[10] != 2 {
            return Err("Bubblewrap supports Type 2 AppImages only; Type 1 is not supported by this sandbox launcher".into());
        }
        return Ok(LaunchTarget {
            destination: "/app/application.AppImage".into(),
            command: vec![
                "/app/application.AppImage".into(),
                "--appimage-extract-and-run".into(),
            ],
        });
    }
    if let Some(loader) = crate::lifecycle::app_file::program_loader(file)? {
        let is_mounted = |path: &Path| {
            super::SYSTEM_RUNTIME
                .iter()
                .any(|root| path.starts_with(root))
                || permissions
                    .paths
                    .iter()
                    .any(|grant| path.starts_with(&grant.path))
        };
        let canonical = std::fs::canonicalize(&loader).map_err(|error| error.to_string())?;
        if !is_mounted(&loader) || !is_mounted(&canonical) {
            return Err(format!("Program loader {} is outside the mounted system runtime and granted folders; grant its folder before launching with Bubblewrap", loader.display()));
        }
    }
    let path = Path::new(&app.file_path);
    let parent = path.parent().ok_or("executable parent folder is missing")?;
    let destination = if permissions
        .paths
        .iter()
        .any(|grant| parent.starts_with(&grant.path))
    {
        app.file_path.clone()
    } else {
        format!(
            "/app/{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .ok_or("invalid executable filename")?
        )
    };
    Ok(LaunchTarget {
        command: vec![destination.clone()],
        destination,
    })
}
