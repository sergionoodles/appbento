//! Refresh library metadata without running imported executables.
use super::{app_digest, file_created_at};
use crate::app_identity::{self, AppIdentity};
use crate::models::{AppKind, InstalledApp};
use chrono::Utc;
use std::fs::File;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub(super) fn path_id(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or("App path must be UTF-8".into())
}

pub(super) fn discovered_app(
    path: &Path,
    file: &File,
    existing: Option<&InstalledApp>,
    identity: AppIdentity,
    kind: AppKind,
) -> Result<InstalledApp, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid filename")?
        .to_owned();
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if let Some(existing) = existing {
        if existing.kind != kind {
            return Err("App format differs from the existing entry; remove the old entry before adding it again".into());
        }
        existing.validate_kind()?;
        let needs_name =
            app_identity::is_legacy_file_name_default(&existing.name, &existing.file_name);
        let has_current_review = existing.launch_config.is_none()
            || existing.reviewed_digest.as_deref() == Some(app_digest(path, kind)?.as_str());
        return Ok(InstalledApp {
            launch_config: existing.launch_config.clone(),
            reviewed_digest: if has_current_review {
                existing.reviewed_digest.clone()
            } else {
                None
            },
            size_bytes: metadata.len(),
            file_created_at: file_created_at(&metadata),
            executable: metadata.permissions().mode() & 0o111 != 0,
            name: if needs_name {
                identity.name
            } else {
                existing.name.clone()
            },
            version_current: existing.version_current.clone().or(identity.version),
            ..existing.clone()
        });
    }
    Ok(InstalledApp {
        kind,
        update_config: None,
        id: path_id(path)?,
        name: identity.name,
        file_path: path_id(path)?,
        file_name,
        size_bytes: metadata.len(),
        file_created_at: file_created_at(&metadata),
        executable: metadata.permissions().mode() & 0o111 != 0,
        integrated: false,
        version_current: identity.version,
        icon_path: None,
        desktop_id: None,
        source_path: None,
        launch_count: 0,
        last_launched: None,
        added_at: Utc::now(),
        launch_config: None,
        reviewed_digest: None,
    })
}

/// Unreadable metadata (Type 1 AppImages, missing squashfs-tools) falls back to the filename.
pub(super) fn read_identity(path: &Path) -> Result<AppIdentity, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid filename")?;
    let embedded = crate::embedded_icon::discover_identity(path).unwrap_or_else(|error| {
        eprintln!(
            "embedded metadata unavailable for {}: {error}",
            path.display()
        );
        Default::default()
    });
    Ok(app_identity::resolve(&embedded, file_name))
}

pub(super) fn read_identity_for_kind(path: &Path, kind: AppKind) -> Result<AppIdentity, String> {
    if kind == AppKind::AppImage {
        return read_identity(path);
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid filename")?;
    Ok(app_identity::resolve(&Default::default(), name))
}
