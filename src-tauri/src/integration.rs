//! Journaled integration keeps the library lock through commit and recovery.
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use super::recovery::{self, Artifact, Journal, Operation, OriginalMode};
use super::{
    candidate_path, discovered_app, is_managed_path, open_appimage, path_id, read_identity,
    self_appimage, Lifecycle,
};
use crate::models::InstalledApp;
use crate::persistence::{self, Repository};
use crate::store::Library;

const EXECUTABLE_MODE: u32 = 0o755;
const NO_FOLLOW: i32 = 0x20000;

#[derive(Clone, Copy)]
enum SourceDisposition {
    Keep,
    Download,
    Move,
}

struct IntegrationPlan {
    journal: Journal,
    app: InstalledApp,
    appimage: Option<Artifact>,
}

impl Lifecycle {
    pub fn import_appimage(
        &self,
        source: &Path,
        move_to_managed: bool,
    ) -> Result<InstalledApp, String> {
        let source = candidate_path(source)?;
        if move_to_managed && !is_managed_path(&self.managed_dir, &source)? {
            return self.integrate(&source, true);
        }
        persistence::with_repository(
            &self.library_path,
            |repository: &mut Repository<Library>| {
                recovery::recover(repository)?;
                let file = open_appimage(&source)?;
                let identity = read_identity(&source)?;
                if self_appimage::is_self_appimage(&source, &identity) {
                    return Err(self_appimage::IMPORT_WARNING.into());
                }
                let id = path_id(&source)?;
                if let Some(existing) = repository.state.values().find(|app| {
                    app.source_path.as_deref() == Some(id.as_str()) && app.file_path != id
                }) {
                    return Ok(existing.clone());
                }
                let existing = repository
                    .state
                    .get(&id)
                    .or_else(|| repository.state.values().find(|app| app.file_path == id));
                let app = InstalledApp {
                    integrated: existing.is_some_and(|app| app.integrated)
                        || is_managed_path(&self.managed_dir, &source)?,
                    ..discovered_app(&source, &file, existing, identity)?
                };
                repository.state.insert(app.id.clone(), app.clone());
                repository.commit().map_err(|error| error.message)?;
                Ok(app)
            },
        )
    }

    pub fn integrate(&self, source: &Path, move_source: bool) -> Result<InstalledApp, String> {
        self.integrate_source(
            source,
            if move_source {
                SourceDisposition::Move
            } else {
                SourceDisposition::Keep
            },
        )
    }

    pub fn integrate_download(&self, source: &Path) -> Result<InstalledApp, String> {
        self.integrate_source(source, SourceDisposition::Download)
    }

    fn integrate_source(
        &self,
        source: &Path,
        disposition: SourceDisposition,
    ) -> Result<InstalledApp, String> {
        persistence::with_repository(
            &self.library_path,
            |repository: &mut Repository<Library>| {
                recovery::recover(repository)?;
                let mut source_file = open_appimage(source)?;
                let plan =
                    self.prepare_integration(source, &source_file, &repository.state, disposition)?;
                let plan = if matches!(disposition, SourceDisposition::Move) {
                    prepare_move(plan, source)?
                } else {
                    plan
                };
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
        disposition: SourceDisposition,
    ) -> Result<IntegrationPlan, String> {
        let source = fs::canonicalize(source).map_err(|error| error.to_string())?;
        let identity = read_identity(&source)?;
        if self_appimage::is_self_appimage(&source, &identity) {
            return Err(self_appimage::IMPORT_WARNING.into());
        }
        fs::create_dir_all(&self.managed_dir).map_err(|error| error.to_string())?;
        let managed = fs::canonicalize(&self.managed_dir).map_err(|error| error.to_string())?;
        let destination = managed.join(source.file_name().ok_or("invalid filename")?);
        if source != destination && recovery::identity(&destination)?.is_some() {
            return Err(
                "destination already exists; replacement requires an explicit operation".into(),
            );
        }
        let source_id = path_id(&source)?;
        let destination_id = path_id(&destination)?;
        let existing = library
            .get(&destination_id)
            .or_else(|| library.get(&source_id))
            .or_else(|| library.values().find(|app| app.file_path == source_id));
        let mut app = discovered_app(&source, source_file, existing, identity)?;
        app.id = existing.map_or_else(|| destination_id.clone(), |app| app.id.clone());
        app.file_path = destination_id.clone();
        app.integrated = true;
        app.executable = !matches!(disposition, SourceDisposition::Move) || app.executable;
        app.source_path = if matches!(disposition, SourceDisposition::Keep) {
            app.source_path
                .or_else(|| (source != destination).then_some(source_id))
        } else {
            None
        };
        let appimage = if source != destination {
            Some(reserve_artifact(&destination)?)
        } else {
            None
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
            artifacts: appimage.iter().cloned().collect(),
            original_mode,
            installed_app: Some(app.clone()),
            previous_library_identity: recovery::identity(&self.library_path)?,
        };
        Ok(IntegrationPlan {
            journal,
            app,
            appimage,
        })
    }
}

fn prepare_move(plan: IntegrationPlan, source: &Path) -> Result<IntegrationPlan, String> {
    let Some(replacement) = &plan.appimage else {
        return Ok(plan);
    };
    let backup = match super::replacement::reserve_backup(source) {
        Ok(backup) => backup,
        Err(error) => {
            recovery::remove_if_present(&replacement.stage)?;
            return Err(error);
        }
    };
    Ok(IntegrationPlan {
        journal: Journal {
            operation: Operation::Replace,
            artifacts: vec![backup, replacement.clone()],
            ..plan.journal
        },
        ..plan
    })
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
        let permissions = if matches!(plan.journal.operation, Operation::Replace) {
            source
                .metadata()
                .map_err(|error| error.to_string())?
                .permissions()
        } else {
            fs::Permissions::from_mode(EXECUTABLE_MODE)
        };
        staged
            .set_permissions(permissions)
            .map_err(|error| error.to_string())?;
        staged.sync_all().map_err(|error| error.to_string())?;
        if matches!(plan.journal.operation, Operation::Replace) {
            let original = &plan.journal.artifacts[0];
            if recovery::identity(&original.original)? != original.identity
                || crate::sandbox::appimage_digest(&original.original)?
                    != crate::sandbox::appimage_digest(&artifact.stage)?
            {
                return Err("AppImage changed while moving; import it again".into());
            }
        }
        fs::hard_link(&artifact.stage, &artifact.original).map_err(|error| error.to_string())?;
        recovery::sync_parent(&artifact.original)?;
    }
    if let Some(original_mode) = &plan.journal.original_mode {
        source
            .set_permissions(fs::Permissions::from_mode(original_mode.mode | 0o100))
            .map_err(|error| error.to_string())?;
        source.sync_all().map_err(|error| error.to_string())?;
    }
    Ok(())
}
