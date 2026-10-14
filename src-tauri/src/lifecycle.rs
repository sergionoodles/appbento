//! Filesystem lifecycle operations, independent of Tauri and injected for tests.
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::app_identity::{self, AppIdentity};
use crate::models::{InstalledApp, ScanReport, ScanWarning};
use crate::persistence;
use crate::store::Library;

#[path = "appimage.rs"]
mod appimage;
#[path = "desktop.rs"]
pub(crate) mod desktop;
#[path = "integration.rs"]
mod integration;
#[path = "menu_integration.rs"]
mod menu_integration;
#[path = "recovery.rs"]
pub(crate) mod recovery;
#[path = "replacement.rs"]
mod replacement;
#[path = "self_appimage.rs"]
mod self_appimage;

pub use appimage::open_appimage;
#[cfg(test)]
use desktop::desktop_content;
#[cfg(test)]
use desktop::desktop_id;

pub struct Lifecycle {
    pub library_path: PathBuf,
    pub managed_dir: PathBuf,
    pub desktop_dir: PathBuf,
}

fn path_id(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or("AppImage path must be UTF-8".into())
}

fn discovered_app(
    path: &Path,
    file: &File,
    existing: Option<&InstalledApp>,
    identity: AppIdentity,
) -> Result<InstalledApp, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid filename")?
        .to_owned();
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if let Some(existing) = existing {
        let needs_name =
            app_identity::is_legacy_file_name_default(&existing.name, &existing.file_name);
        let has_current_review = existing.launch_config.is_none()
            || existing.reviewed_digest.as_deref()
                == Some(crate::sandbox::appimage_digest(path)?.as_str());
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
fn read_identity(path: &Path) -> Result<AppIdentity, String> {
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

fn file_created_at(metadata: &fs::Metadata) -> Option<DateTime<Utc>> {
    metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()
        .map(DateTime::from)
}

impl Lifecycle {
    pub fn configure(
        &self,
        id: &str,
        config: &crate::models::LaunchConfig,
    ) -> Result<InstalledApp, String> {
        let config = crate::sandbox::validate_config(
            config,
            self.library_path.parent().ok_or("invalid policy path")?,
        )?;
        persistence::with_repository(
            &self.library_path,
            |repository: &mut persistence::Repository<Library>| {
                recovery::recover(repository)?;
                let app = repository.state.get_mut(id).ok_or("app not found")?;
                app.reviewed_digest =
                    Some(crate::sandbox::appimage_digest(Path::new(&app.file_path))?);
                app.launch_config = Some(config);
                let configured = app.clone();
                if let Some(path) = desktop::entry_path(&self.desktop_dir, &configured)? {
                    desktop::migrate(&path, &configured)?;
                }
                repository.commit().map_err(|error| error.message)?;
                Ok(configured)
            },
        )
    }

    pub fn list(&self) -> Result<Vec<InstalledApp>, String> {
        let apps = Self::list_at(&self.library_path)?;
        for app in &apps {
            if let Some(path) = desktop::entry_path(&self.desktop_dir, app)? {
                desktop::migrate(&path, app)?;
            }
        }
        Ok(apps)
    }

    pub fn list_at(path: &Path) -> Result<Vec<InstalledApp>, String> {
        persistence::with_repository(path, |repository: &mut persistence::Repository<Library>| {
            recovery::recover(repository)?;
            let mut apps: Vec<_> = repository.state.values().cloned().collect();
            apps.sort_by_key(|app| std::cmp::Reverse(app.last_launched));
            Ok(apps)
        })
    }

    pub fn scan(&self, directories: Vec<PathBuf>) -> Result<ScanReport, String> {
        persistence::with_repository(
            &self.library_path,
            |repository: &mut persistence::Repository<Library>| {
                recovery::recover(repository)?;
                let library = &mut repository.state;
                let mut found = Vec::new();
                let mut warnings = Vec::new();
                for directory in directories {
                    let entries = match fs::read_dir(&directory) {
                        Ok(entries) => entries,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(error) => {
                            return Err(format!("cannot scan {}: {error}", directory.display()))
                        }
                    };
                    for entry in entries {
                        let path = match entry {
                            Ok(entry) => entry.path(),
                            Err(error) => {
                                warnings.push(ScanWarning {
                                    path: directory.to_string_lossy().into_owned(),
                                    message: error.to_string(),
                                });
                                continue;
                            }
                        };
                        if !path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("AppImage"))
                        {
                            continue;
                        }
                        let path = match candidate_path(&path) {
                            Ok(path) => path,
                            Err(message) => {
                                warnings.push(ScanWarning {
                                    path: path.to_string_lossy().into_owned(),
                                    message,
                                });
                                continue;
                            }
                        };
                        if library.values().any(|app| {
                            (app.source_path.as_deref() == path.to_str()
                                || app.id == path.to_string_lossy())
                                && app.file_path != path.to_string_lossy()
                        }) {
                            continue;
                        }
                        let file = match open_appimage(&path) {
                            Ok(file) => file,
                            Err(message) => {
                                warnings.push(ScanWarning {
                                    path: path.to_string_lossy().into_owned(),
                                    message,
                                });
                                continue;
                            }
                        };
                        let id = path_id(&path)?;
                        let identity = read_identity(&path)?;
                        if self_appimage::is_self_appimage(&path, &identity) {
                            library.retain(|_, app| app.file_path != id);
                            continue;
                        }
                        let existing = library
                            .get(&id)
                            .or_else(|| library.values().find(|app| app.file_path == id));
                        let app = discovered_app(&path, &file, existing, identity)?;
                        library.insert(app.id.clone(), app.clone());
                        if !found
                            .iter()
                            .any(|existing: &InstalledApp| existing.id == app.id)
                        {
                            found.push(app);
                        }
                    }
                }
                repository.commit().map_err(|error| error.message)?;
                Ok(ScanReport {
                    apps: found,
                    warnings,
                })
            },
        )
    }

    pub fn remove(&self, id: &str, delete_file: bool) -> Result<(), String> {
        persistence::with_repository(
            &self.library_path,
            |repository: &mut persistence::Repository<Library>| {
                recovery::recover(repository)?;
                let app = repository.state.get(id).ok_or("app not found")?;
                let path = PathBuf::from(&app.file_path);
                let desktop_path = desktop::entry_path(&self.desktop_dir, app)?;
                if delete_file && !app.integrated {
                    return Err("only owned integrated AppImages can be deleted".into());
                }
                if let Some(desktop) = &desktop_path {
                    desktop::migrate(desktop, app)?;
                }
                let mut artifacts = Vec::new();
                if delete_file {
                    let _file = open_appimage(&path)?;
                    artifacts.push(removal_artifact(&path)?);
                }
                if let Some(desktop) = desktop_path {
                    match fs::read_to_string(&desktop) {
                        Ok(content) => {
                            let expected =
                                desktop::desktop_content_for_app(&path, &app.id, &app.name)?;
                            let expected_exec = expected
                                .lines()
                                .find(|line| line.starts_with("Exec="))
                                .ok_or("missing Exec")?;
                            if !content
                                .lines()
                                .any(|line| line == "X-AppBento-Managed=true")
                                || !content.lines().any(|line| line == expected_exec)
                            {
                                return Err("desktop entry ownership cannot be verified".into());
                            }
                            artifacts.push(removal_artifact(&desktop)?);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
                let journal = recovery::Journal {
                    operation: recovery::Operation::Remove,
                    app_id: id.to_owned(),
                    artifacts,
                    original_mode: None,
                    installed_app: None,
                    previous_library_identity: recovery::identity(&self.library_path)?,
                };
                recovery::write(&self.library_path, &journal)?;
                let staging = journal.artifacts.iter().try_for_each(stage_removal);
                if let Err(error) = staging {
                    recovery::finish(&self.library_path, &journal, false)
                        .map_err(|cleanup| format!("{error}; recovery required: {cleanup}"))?;
                    return Err(error);
                }
                repository.state.remove(id);
                match repository.commit() {
                    Ok(()) => recovery::finish(&self.library_path, &journal, true),
                    Err(error) => {
                        recovery::finish(&self.library_path, &journal, error.committed).map_err(
                            |cleanup| format!("{}; recovery required: {cleanup}", error.message),
                        )?;
                        Err(error.message)
                    }
                }
            },
        )
    }
}

fn candidate_path(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() {
        return Err("AppImage must be a regular non-symlink file".into());
    }
    fs::canonicalize(path).map_err(|error| error.to_string())
}

fn removal_artifact(path: &Path) -> Result<recovery::Artifact, String> {
    let parent = path.parent().ok_or("invalid removal path")?;
    loop {
        let stage = persistence::temporary_path(parent);
        if !stage.try_exists().map_err(|error| error.to_string())? {
            return Ok(recovery::Artifact {
                original: path.to_owned(),
                stage,
                identity: recovery::identity(path)?,
            });
        }
    }
}

fn stage_removal(artifact: &recovery::Artifact) -> Result<(), String> {
    fs::hard_link(&artifact.original, &artifact.stage).map_err(|error| error.to_string())?;
    recovery::sync_parent(&artifact.stage)?;
    fs::remove_file(&artifact.original).map_err(|error| error.to_string())?;
    recovery::sync_parent(&artifact.original)
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "update_metadata_tests.rs"]
mod update_metadata_tests;
