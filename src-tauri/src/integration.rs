//! Journaled integration keeps the library lock through commit and recovery.
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use super::desktop::desktop_content_with_hints;
use super::recovery::{self, Artifact, Journal, Operation, OriginalMode};
use super::{desktop_id, discovered_app, open_appimage, path_id, Lifecycle};
use crate::models::InstalledApp;
use crate::persistence::{self, Repository};
use crate::store::Library;

const EXECUTABLE_MODE: u32 = 0o755;
const NO_FOLLOW: i32 = 0x20000;

struct IntegrationPlan {
    journal: Journal,
    app: InstalledApp,
    appimage: Option<Artifact>,
    desktop: Option<Artifact>,
    desktop_content: String,
}

impl Lifecycle {
    pub fn integrate(&self, source: &Path) -> Result<InstalledApp, String> {
        persistence::with_repository(
            &self.library_path,
            |repository: &mut Repository<Library>| {
                recovery::recover(repository)?;
                let mut source_file = open_appimage(source)?;
                let plan = self.prepare_integration(source, &source_file, &repository.state)?;
                if let Err(error) = recovery::write(&self.library_path, &plan.journal) {
                    recovery::finish(&self.library_path, &plan.journal, false)
                        .map_err(|cleanup| format!("{error}; recovery required: {cleanup}"))?;
                    return Err(error);
                }
                if let Err(error) = install_artifacts(&plan, &mut source_file) {
                    recovery::finish(&self.library_path, &plan.journal, false)
                        .map_err(|cleanup| format!("{error}; recovery required: {cleanup}"))?;
                    return Err(error);
                }
                if let Some(source_path) = &plan.app.source_path {
                    repository.state.remove(source_path);
                }
                repository
                    .state
                    .insert(plan.app.id.clone(), plan.app.clone());
                match repository.commit() {
                    Ok(()) => {
                        recovery::finish(&self.library_path, &plan.journal, true)?;
                        Ok(plan.app)
                    }
                    Err(error) => {
                        recovery::finish(&self.library_path, &plan.journal, error.committed)
                            .map_err(|cleanup| {
                                format!("{}; recovery required: {cleanup}", error.message)
                            })?;
                        Err(error.message)
                    }
                }
            },
        )
    }

    fn prepare_integration(
        &self,
        source: &Path,
        source_file: &File,
        library: &Library,
    ) -> Result<IntegrationPlan, String> {
        let source = fs::canonicalize(source).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.managed_dir).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.desktop_dir).map_err(|error| error.to_string())?;
        let managed = fs::canonicalize(&self.managed_dir).map_err(|error| error.to_string())?;
        let destination = managed.join(source.file_name().ok_or("invalid filename")?);
        if destination.exists() && source != destination {
            return Err(
                "destination already exists; replacement requires an explicit operation".into(),
            );
        }
        let source_id = path_id(&source)?;
        let destination_id = path_id(&destination)?;
        let existing = library
            .get(&destination_id)
            .or_else(|| library.get(&source_id));
        let mut app = discovered_app(&source, source_file, &self.catalog, existing)?;
        app.id = destination_id.clone();
        app.file_path = destination_id.clone();
        app.integrated = true;
        app.executable = true;
        app.desktop_id = Some(desktop_id(&destination));
        if source != destination {
            app.source_path = Some(source_id);
        }
        let desktop_path = self
            .desktop_dir
            .join(app.desktop_id.as_ref().ok_or("desktop id unavailable")?);
        let hints = app
            .catalog_id
            .as_deref()
            .and_then(|id| self.catalog.iter().find(|entry| entry.app.id == id))
            .map(|entry| entry.desktop.clone())
            .unwrap_or_default();
        if hints
            .categories
            .iter()
            .chain(hints.mime_type.iter())
            .any(|value| value.chars().any(char::is_control))
        {
            return Err("desktop hints contain control characters".into());
        }
        if let Some(existing) = existing.filter(|app| app.integrated) {
            super::desktop::migrate(&desktop_path, existing)?;
        }
        let content = desktop_content_with_hints(&destination, &app.name, &hints)?;
        let is_idempotent = source == destination
            && existing.is_some_and(|app| app.integrated)
            && desktop_path.exists();
        if is_idempotent
            && fs::read_to_string(&desktop_path).map_err(|error| error.to_string())? != content
        {
            return Err("existing desktop entry ownership or metadata differs".into());
        }
        let appimage = if source != destination {
            Some(reserve_artifact(&destination)?)
        } else {
            None
        };
        let desktop = if is_idempotent {
            None
        } else {
            match reserve_artifact(&desktop_path) {
                Ok(artifact) => Some(artifact),
                Err(error) => {
                    if let Some(artifact) = &appimage {
                        recovery::remove_if_present(&artifact.stage)?;
                    }
                    return Err(error);
                }
            }
        };
        let original_mode = if source == destination {
            Some(OriginalMode {
                path: source,
                mode: source_file
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .permissions()
                    .mode(),
            })
        } else {
            None
        };
        let journal = Journal {
            operation: Operation::Integrate,
            app_id: app.id.clone(),
            artifacts: appimage.iter().chain(desktop.iter()).cloned().collect(),
            original_mode,
            installed_app: Some(app.clone()),
            previous_library_identity: recovery::identity(&self.library_path)?,
        };
        Ok(IntegrationPlan {
            journal,
            app,
            appimage,
            desktop,
            desktop_content: content,
        })
    }
}

fn reserve_artifact(original: &Path) -> Result<Artifact, String> {
    if original.try_exists().map_err(|error| error.to_string())? {
        return Err(format!(
            "destination already exists: {}",
            original.display()
        ));
    }
    let (stage, file) =
        persistence::create_temporary(original.parent().ok_or("invalid artifact path")?)?;
    file.sync_all().map_err(|error| error.to_string())?;
    recovery::sync_parent(&stage)?;
    Ok(Artifact {
        original: original.to_owned(),
        identity: recovery::identity(&stage)?,
        stage,
    })
}

fn stage_file(artifact: &Artifact) -> Result<File, String> {
    if recovery::identity(&artifact.stage)? != artifact.identity {
        return Err(format!(
            "staging file was replaced: {}",
            artifact.stage.display()
        ));
    }
    OpenOptions::new()
        .write(true)
        .custom_flags(NO_FOLLOW)
        .open(&artifact.stage)
        .map_err(|error| error.to_string())
}

fn install_artifacts(plan: &IntegrationPlan, source: &mut File) -> Result<(), String> {
    if let Some(artifact) = &plan.appimage {
        let mut staged = stage_file(artifact)?;
        std::io::copy(source, &mut staged).map_err(|error| error.to_string())?;
        staged
            .set_permissions(fs::Permissions::from_mode(EXECUTABLE_MODE))
            .map_err(|error| error.to_string())?;
        staged.sync_all().map_err(|error| error.to_string())?;
        fs::hard_link(&artifact.stage, &artifact.original).map_err(|error| error.to_string())?;
        recovery::sync_parent(&artifact.original)?;
    }
    if let Some(original_mode) = &plan.journal.original_mode {
        source
            .set_permissions(fs::Permissions::from_mode(original_mode.mode | 0o100))
            .map_err(|error| error.to_string())?;
        source.sync_all().map_err(|error| error.to_string())?;
    }
    if let Some(artifact) = &plan.desktop {
        let mut staged = stage_file(artifact)?;
        staged
            .write_all(plan.desktop_content.as_bytes())
            .map_err(|error| error.to_string())?;
        staged
            .set_permissions(fs::Permissions::from_mode(0o644))
            .map_err(|error| error.to_string())?;
        staged.sync_all().map_err(|error| error.to_string())?;
        fs::hard_link(&artifact.stage, &artifact.original).map_err(|error| error.to_string())?;
        recovery::sync_parent(&artifact.original)?;
    }
    Ok(())
}
