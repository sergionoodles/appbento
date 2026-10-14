//! Explicit launch-menu changes leave AppImage ownership and location untouched.
use std::fs;
use std::io::Write;
use std::path::Path;

use super::{desktop, open_appimage, recovery, removal_artifact, stage_removal, Lifecycle};
use crate::models::InstalledApp;
use crate::persistence::{self, Repository};
use crate::store::Library;

impl Lifecycle {
    pub fn set_menu_entry(&self, id: &str, enabled: bool) -> Result<InstalledApp, String> {
        persistence::with_repository(
            &self.library_path,
            |repository: &mut Repository<Library>| {
                recovery::recover(repository)?;
                let mut app = repository.state.get(id).ok_or("app not found")?.clone();
                let desktop_id = app
                    .desktop_id
                    .clone()
                    .unwrap_or_else(|| desktop::desktop_id(Path::new(&app.file_path)));
                let destination = desktop::entry_path(&self.desktop_dir, &app)?
                    .unwrap_or_else(|| self.desktop_dir.join(&desktop_id));
                let artifact = if enabled {
                    open_appimage(Path::new(&app.file_path))?;
                    if app.desktop_id.is_some() && recovery::identity(&destination)?.is_some() {
                        desktop::migrate(&destination, &app)?;
                        return Ok(app);
                    }
                    Some(create_menu_artifact(&self.desktop_dir, &destination, &app)?)
                } else {
                    if app.desktop_id.is_none() {
                        return Ok(app);
                    }
                    if recovery::identity(&destination)?.is_some() {
                        desktop::migrate(&destination, &app)?;
                        Some(removal_artifact(&destination)?)
                    } else {
                        None
                    }
                };
                app.desktop_id = enabled.then_some(desktop_id);
                let journal = recovery::Journal {
                    operation: if enabled {
                        recovery::Operation::MenuCreate
                    } else {
                        recovery::Operation::MenuRemove
                    },
                    app_id: id.to_owned(),
                    artifacts: artifact.into_iter().collect(),
                    original_mode: None,
                    installed_app: Some(app.clone()),
                    previous_library_identity: recovery::identity(&self.library_path)?,
                };
                if let Err(error) = recovery::write(&self.library_path, &journal) {
                    recovery::finish(&self.library_path, &journal, false)?;
                    return Err(error);
                }
                let installation = journal.artifacts.iter().try_for_each(|artifact| {
                    if enabled {
                        fs::hard_link(&artifact.stage, &artifact.original)
                            .map_err(|error| error.to_string())?;
                        recovery::sync_parent(&artifact.original)
                    } else {
                        stage_removal(artifact)
                    }
                });
                if let Err(error) = installation {
                    recovery::finish(&self.library_path, &journal, false)?;
                    return Err(error);
                }
                repository.state.insert(id.to_owned(), app.clone());
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

fn create_menu_artifact(
    directory: &Path,
    destination: &Path,
    app: &InstalledApp,
) -> Result<recovery::Artifact, String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    if recovery::identity(destination)?.is_some() {
        return Err("a desktop entry already exists; it will not be overwritten".into());
    }
    let content = desktop::content_for_entry(app)?;
    let (stage, mut file) = persistence::create_temporary(directory)?;
    let preparation = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string());
    if let Err(error) = preparation {
        recovery::remove_if_present(&stage)?;
        return Err(error);
    }
    Ok(recovery::Artifact {
        original: destination.to_owned(),
        identity: recovery::identity(&stage)?,
        stage,
    })
}
