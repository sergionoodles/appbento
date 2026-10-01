//! Filesystem lifecycle operations, independent of Tauri and injected for tests.
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::catalog;
use crate::models::{CatalogEntry, InstalledApp, ScanReport, ScanWarning};
use crate::persistence;
use crate::store::Library;

#[path = "appimage.rs"]
mod appimage;
#[path = "desktop.rs"]
mod desktop;
#[path = "integration.rs"]
mod integration;
#[path = "recovery.rs"]
pub(crate) mod recovery;
#[path = "replacement.rs"]
mod replacement;

pub use appimage::open_appimage;
#[cfg(test)]
use desktop::desktop_content;
use desktop::desktop_id;

pub struct Lifecycle {
    pub library_path: PathBuf,
    pub managed_dir: PathBuf,
    pub desktop_dir: PathBuf,
    pub catalog: Vec<CatalogEntry>,
}

fn path_id(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or("AppImage path must be UTF-8".into())
}

fn discovered_app(
    path: &Path,
    file: &File,
    entries: &[CatalogEntry],
    existing: Option<&InstalledApp>,
) -> Result<InstalledApp, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid filename")?
        .to_owned();
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if let Some(existing) = existing {
        let has_current_review = existing.launch_config.is_none()
            || existing.reviewed_digest.as_deref()
                == Some(crate::sandbox::appimage_digest(path)?.as_str());
        return Ok(InstalledApp {
            launch_config: if has_current_review {
                existing.launch_config.clone()
            } else {
                None
            },
            reviewed_digest: if has_current_review {
                existing.reviewed_digest.clone()
            } else {
                None
            },
            size_bytes: metadata.len(),
            file_created_at: file_created_at(&metadata),
            executable: metadata.permissions().mode() & 0o111 != 0,
            ..existing.clone()
        });
    }
    let catalog_id = catalog::match_catalog(entries, &file_name);
    let entry = catalog_id
        .as_ref()
        .and_then(|id| entries.iter().find(|entry| &entry.app.id == id));
    let version = catalog_id
        .as_deref()
        .and_then(|id| catalog::version_from_filename(entries, id, &file_name));
    Ok(InstalledApp {
        update_config: entry.and_then(|entry| {
            if matches!(
                entry.update.strategy,
                Some(crate::models::UpdateStrategy::GithubRelease)
            ) {
                entry
                    .update
                    .github
                    .clone()
                    .map(crate::models::UpdateConfig::GithubRelease)
            } else {
                None
            }
        }),
        permission_defaults: entry.and_then(|entry| entry.sandbox.clone()),
        id: path_id(path)?,
        name: catalog_id
            .as_ref()
            .and_then(|id| entries.iter().find(|entry| &entry.app.id == id))
            .map(|entry| entry.app.name.clone())
            .unwrap_or_else(|| {
                file_name
                    .trim_end_matches(".AppImage")
                    .replace(['-', '_'], " ")
            }),
        file_path: path_id(path)?,
        file_name,
        size_bytes: metadata.len(),
        file_created_at: file_created_at(&metadata),
        executable: metadata.permissions().mode() & 0o111 != 0,
        integrated: false,
        catalog_id,
        version_current: version,
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
                repository.commit().map_err(|error| error.message)?;
                Ok(configured)
            },
        )
    }

    pub fn list(&self) -> Result<Vec<InstalledApp>, String> {
        let apps = Self::list_at(&self.library_path)?;
        for app in &apps {
            if app.integrated {
                desktop::migrate(
                    &self.desktop_dir.join(desktop_id(Path::new(&app.file_path))),
                    app,
                )?;
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
                        let existing = library
                            .get(&id)
                            .or_else(|| library.values().find(|app| app.file_path == id));
                        let app = discovered_app(&path, &file, &self.catalog, existing)?;
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
                let expected_desktop_id = desktop_id(&path);
                if delete_file
                    && (!app.integrated || app.desktop_id.as_deref() != Some(&expected_desktop_id))
                {
                    return Err("only owned integrated AppImages can be deleted".into());
                }
                if app.integrated {
                    desktop::migrate(&self.desktop_dir.join(&expected_desktop_id), app)?;
                }
                let mut artifacts = Vec::new();
                if delete_file {
                    let _file = open_appimage(&path)?;
                    artifacts.push(removal_artifact(&path)?);
                }
                if app.integrated {
                    let desktop = self.desktop_dir.join(&expected_desktop_id);
                    match fs::read_to_string(&desktop) {
                        Ok(content) => {
                            let expected = desktop::desktop_content_with_hints(
                                &path,
                                &app.id,
                                &app.name,
                                &Default::default(),
                            )?;
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
