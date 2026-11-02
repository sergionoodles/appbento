//! Filesystem lifecycle operations, independent of Tauri and injected for tests.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::models::{AppKind, InstalledApp, ScanReport, ScanWarning};
use crate::persistence;
use crate::store::Library;

#[path = "app_discovery.rs"]
mod app_discovery;
use app_discovery::{discovered_app, path_id, read_identity};
#[path = "app_file.rs"]
pub(crate) mod app_file;
#[path = "appimage.rs"]
mod appimage;
#[path = "desktop.rs"]
pub(crate) mod desktop;
#[path = "executable.rs"]
mod executable;
#[path = "integration.rs"]
mod integration;
#[path = "local_import.rs"]
mod local_import;
#[path = "menu_integration.rs"]
mod menu_integration;
#[path = "recovery.rs"]
pub(crate) mod recovery;
#[path = "replacement.rs"]
mod replacement;
#[path = "self_appimage.rs"]
mod self_appimage;

pub use app_file::{app_digest, digest_file, open_app};
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

fn file_created_at(metadata: &fs::Metadata) -> Option<DateTime<Utc>> {
    metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()
        .map(DateTime::from)
}

impl Lifecycle {
    pub fn prepare_launch(&self, id: &str, make_executable: bool) -> Result<InstalledApp, String> {
        const OWNER_EXECUTE: u32 = 0o100;
        const EXECUTE_BITS: u32 = 0o111;

        persistence::with_repository(
            &self.library_path,
            |repository: &mut persistence::Repository<Library>| {
                recovery::recover(repository)?;
                let app = repository.state.get_mut(id).ok_or("app not found")?;
                let file = open_app(Path::new(&app.file_path), app.kind)?;
                let mut permissions = file
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .permissions();
                if make_executable && permissions.mode() & EXECUTE_BITS == 0 {
                    permissions.set_mode(permissions.mode() | OWNER_EXECUTE);
                    file.set_permissions(permissions.clone())
                        .map_err(|error| format!("Cannot make app executable: {error}"))?;
                }
                app.executable = permissions.mode() & EXECUTE_BITS != 0;
                let prepared = app.clone();
                repository.commit().map_err(|error| error.message)?;
                Ok(prepared)
            },
        )
    }

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
                app.reviewed_digest = Some(app_digest(Path::new(&app.file_path), app.kind)?);
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
                        if library
                            .values()
                            .any(|app| app.file_path == id && app.kind != AppKind::AppImage)
                        {
                            warnings.push(ScanWarning { path: id, message: "Saved app kind differs from the scanned AppImage; remove the old entry before adding it again".into() });
                            continue;
                        }
                        let identity = read_identity(&path)?;
                        if self_appimage::is_self_appimage(&path, &identity) {
                            library.retain(|_, app| app.file_path != id);
                            continue;
                        }
                        let existing = library
                            .get(&id)
                            .or_else(|| library.values().find(|app| app.file_path == id));
                        let app = InstalledApp {
                            integrated: existing.is_some_and(|app| app.integrated)
                                || is_managed_path(&self.managed_dir, &path)?,
                            ..discovered_app(&path, &file, existing, identity, AppKind::AppImage)?
                        };
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
                if delete_file && (app.kind == AppKind::Binary || !app.integrated) {
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
        return Err("App file must be a regular non-symlink file".into());
    }
    fs::canonicalize(path).map_err(|error| error.to_string())
}

fn is_managed_path(managed_dir: &Path, path: &Path) -> Result<bool, String> {
    let managed = match fs::canonicalize(managed_dir) {
        Ok(managed) => managed,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    Ok(path.parent() == Some(managed.as_path()))
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

#[cfg(test)]
#[path = "update_path_tests.rs"]
mod update_path_tests;

#[cfg(test)]
#[path = "legacy_update_tests.rs"]
mod legacy_update_tests;

#[cfg(test)]
#[path = "import_tests.rs"]
mod import_tests;

#[cfg(test)]
#[path = "executable_tests.rs"]
pub(crate) mod executable_tests;
