//! Manual updates retain library identity and recover interrupted file replacement.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{file_created_at, open_appimage, recovery, Lifecycle};
use crate::models::InstalledApp;
use crate::persistence::{self, Repository};
use crate::store::Library;

const EXECUTABLE_MODE: u32 = 0o755;

impl Lifecycle {
    pub fn replace_update(
        &self,
        expected: &InstalledApp,
        staged: &Path,
        latest: Option<&str>,
        expected_digest: &str,
    ) -> Result<InstalledApp, String> {
        let mut download = open_appimage(staged)?;
        persistence::with_repository(
            &self.library_path,
            |repository: &mut Repository<Library>| {
                recovery::recover(repository)?;
                let current = repository.state.get(&expected.id).ok_or("app not found")?;
                validate_snapshot(expected, current, expected_digest)?;
                let directory = self
                    .library_path
                    .parent()
                    .ok_or("invalid library path")?
                    .join("updates");
                let destination = update_destination(current, &directory)?;
                if destination != Path::new(&current.file_path)
                    && destination
                        .try_exists()
                        .map_err(|error| error.to_string())?
                {
                    return Err("update destination already exists".into());
                }
                let icon_directory = self
                    .library_path
                    .parent()
                    .ok_or("invalid library path")?
                    .join("icons");
                let (temporary, app) = stage_update(
                    &mut download,
                    current,
                    &destination,
                    latest,
                    &icon_directory,
                )?;
                let preparation =
                    validate_snapshot(expected, current, expected_digest).and_then(|_| {
                        prepare_journal(
                            &self.library_path,
                            &destination,
                            &temporary,
                            app,
                            destination == Path::new(&current.file_path),
                        )
                    });
                let journal = match preparation {
                    Ok(journal) => journal,
                    Err(error) => {
                        recovery::remove_if_present(&temporary)?;
                        return Err(error);
                    }
                };
                if let Err(error) = recovery::write(&self.library_path, &journal) {
                    recovery::finish(&self.library_path, &journal, false)?;
                    return Err(error);
                }
                let installation = if matches!(journal.operation, recovery::Operation::Replace) {
                    fs::rename(&temporary, &destination)
                } else {
                    fs::hard_link(&temporary, &destination)
                }
                .map_err(|error| error.to_string())
                .and_then(|_| recovery::sync_parent(&destination));
                if let Err(error) = installation {
                    recovery::finish(&self.library_path, &journal, false)?;
                    return Err(error);
                }
                let app = journal
                    .installed_app
                    .as_ref()
                    .ok_or("missing updated app")?
                    .clone();
                repository.state.insert(app.id.clone(), app.clone());
                match repository.commit() {
                    Ok(()) => {
                        recovery::finish(&self.library_path, &journal, true)?;
                        Ok(app)
                    }
                    Err(error) => {
                        recovery::finish(&self.library_path, &journal, error.committed)?;
                        Err(error.message)
                    }
                }
            },
        )
    }
}

fn validate_snapshot(
    expected: &InstalledApp,
    current: &InstalledApp,
    expected_digest: &str,
) -> Result<(), String> {
    if expected.update_config != current.update_config
        || expected.file_path != current.file_path
        || expected.version_current != current.version_current
    {
        return Err("app configuration changed while downloading; check for updates again".into());
    }
    let file = open_appimage(Path::new(&current.file_path))?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if metadata.len() != expected.size_bytes
        || crate::sandbox::appimage_digest(Path::new(&current.file_path))? != expected_digest
    {
        return Err("AppImage changed while downloading; check for updates again".into());
    }
    Ok(())
}

fn update_destination(app: &InstalledApp, directory: &Path) -> Result<PathBuf, String> {
    let current = Path::new(&app.file_path);
    if app.integrated {
        if app.desktop_id.as_deref() != Some(super::desktop_id(current).as_str()) {
            return Err("integrated AppImage ownership cannot be verified".into());
        }
        return Ok(current.to_owned());
    }
    if current.parent() == Some(directory) {
        return Ok(current.to_owned());
    }
    Ok(directory.join(format!("{:x}.AppImage", Sha256::digest(app.id.as_bytes()))))
}

fn prepare_journal(
    library: &Path,
    destination: &Path,
    temporary: &Path,
    app: InstalledApp,
    is_owned_destination: bool,
) -> Result<recovery::Journal, String> {
    let replacement = recovery::Artifact {
        original: destination.to_owned(),
        stage: temporary.to_owned(),
        identity: recovery::identity(temporary)?,
    };
    let (operation, artifacts) = if is_owned_destination {
        let backup = reserve_backup(destination)?;
        (recovery::Operation::Replace, vec![backup, replacement])
    } else {
        (recovery::Operation::Integrate, vec![replacement])
    };
    Ok(recovery::Journal {
        operation,
        app_id: app.id.clone(),
        artifacts,
        original_mode: None,
        installed_app: Some(app),
        previous_library_identity: recovery::identity(library)?,
    })
}

fn reserve_backup(destination: &Path) -> Result<recovery::Artifact, String> {
    loop {
        let stage = persistence::temporary_path(destination.parent().ok_or("invalid destination")?);
        match fs::hard_link(destination, &stage) {
            Ok(()) => {
                recovery::sync_parent(&stage)?;
                return Ok(recovery::Artifact {
                    original: destination.to_owned(),
                    identity: recovery::identity(&stage)?,
                    stage,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn stage_update(
    download: &mut fs::File,
    current: &InstalledApp,
    destination: &Path,
    latest: Option<&str>,
    icon_directory: &Path,
) -> Result<(PathBuf, InstalledApp), String> {
    let parent = destination.parent().ok_or("invalid update destination")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let (temporary, mut file) = persistence::create_temporary(parent)?;
    let preparation = (|| {
        std::io::copy(download, &mut file).map_err(|error| error.to_string())?;
        file.set_permissions(fs::Permissions::from_mode(EXECUTABLE_MODE))
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        let validated = open_appimage(&temporary)?;
        let metadata = validated.metadata().map_err(|error| error.to_string())?;
        let app = InstalledApp {
            file_path: destination
                .to_str()
                .ok_or("update path must be UTF-8")?
                .into(),
            source_path: current.source_path.clone().or_else(|| {
                (destination != Path::new(&current.file_path)).then(|| current.file_path.clone())
            }),
            file_name: destination
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("invalid update filename")?
                .into(),
            size_bytes: metadata.len(),
            file_created_at: file_created_at(&metadata),
            executable: true,
            reviewed_digest: current
                .launch_config
                .as_ref()
                .map(|_| crate::sandbox::appimage_digest(&temporary))
                .transpose()?,
            ..current.clone()
        };

        refresh_metadata(app, &temporary, latest, icon_directory)
    })();
    match preparation {
        Ok(app) => Ok((temporary, app)),
        Err(error) => {
            recovery::remove_if_present(&temporary)?;
            Err(error)
        }
    }
}

fn refresh_metadata(
    app: InstalledApp,
    path: &Path,
    latest: Option<&str>,
    icon_directory: &Path,
) -> Result<InstalledApp, String> {
    let embedded = crate::embedded_icon::discover_identity(path).unwrap_or_else(|error| {
        eprintln!("update metadata unavailable for {}: {error}", app.id);
        Default::default()
    });
    // Staging and managed filenames do not describe the new release's identity.
    let identity = crate::app_identity::resolve(&embedded, "");
    let metadata = crate::embedded_icon::discover(path).unwrap_or_else(|error| {
        eprintln!("update icon unavailable for {}: {error}", app.id);
        Default::default()
    });
    let icon_path = match metadata.icon {
        Some(bytes) => Some(
            crate::icons::save(icon_directory, &bytes)?
                .to_str()
                .ok_or("icon path must be UTF-8")?
                .to_owned(),
        ),
        None => app.icon_path,
    };

    Ok(InstalledApp {
        name: if identity.name.is_empty() {
            app.name
        } else {
            identity.name
        },
        version_current: latest
            .map(str::to_owned)
            .or(identity.version)
            .or(app.version_current),
        icon_path,
        ..app
    })
}
