//! Durable intent journals and no-clobber lifecycle recovery under the library lock.
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::persistence::{self, Repository};
use crate::store::Library;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Integrate,
    Remove,
    Replace,
    MenuCreate,
    MenuRemove,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub original: PathBuf,
    pub stage: PathBuf,
    #[serde(default)]
    pub identity: Option<(u64, u64)>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OriginalMode {
    pub path: PathBuf,
    pub mode: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Journal {
    pub operation: Operation,
    pub app_id: String,
    pub artifacts: Vec<Artifact>,
    pub original_mode: Option<OriginalMode>,
    #[serde(default)]
    pub installed_app: Option<crate::models::InstalledApp>,
    #[serde(default)]
    pub previous_library_identity: Option<(u64, u64)>,
}

fn journal_path(library: &Path) -> PathBuf {
    library.with_extension("lifecycle.json")
}

pub fn write(library: &Path, journal: &Journal) -> Result<(), String> {
    persistence::write_atomic(
        &journal_path(library),
        &serde_json::to_vec(journal).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.message)
}

pub fn recover(repository: &mut Repository<Library>) -> Result<(), String> {
    let bytes = match fs::read(journal_path(&repository.path)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("cannot read lifecycle recovery journal: {error}")),
    };
    let journal: Journal = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid lifecycle recovery journal: {error}"))?;
    let committed = match journal.operation {
        Operation::Integrate
        | Operation::Replace
        | Operation::MenuCreate
        | Operation::MenuRemove => {
            identity(&repository.path)? != journal.previous_library_identity
                && journal.installed_app.as_ref().is_some_and(|expected| {
                    repository
                        .state
                        .get(&journal.app_id)
                        .is_some_and(|actual| actual == expected)
                })
        }
        Operation::Remove => !repository.state.contains_key(&journal.app_id),
    };
    finish(&repository.path, &journal, committed)
}

pub fn finish(library: &Path, journal: &Journal, committed: bool) -> Result<(), String> {
    if matches!(journal.operation, Operation::Replace) {
        finish_replacement(journal, committed)?;
        remove_if_present(&journal_path(library))?;
        return sync_parent(library);
    }
    for artifact in &journal.artifacts {
        let stage = identity(&artifact.stage)?;
        if artifact.identity.is_some() && stage.is_some() && stage != artifact.identity {
            return Err(format!(
                "recovery conflict: staging file {} was replaced",
                artifact.stage.display()
            ));
        }
        match (&journal.operation, committed) {
            (Operation::Integrate | Operation::MenuCreate, false) => {
                remove_created_artifact(artifact)?
            }
            (Operation::Remove | Operation::MenuRemove, false) => {
                restore_removed_artifact(artifact)?
            }
            _ => remove_if_present(&artifact.stage)?,
        }
        sync_parent(&artifact.original)?;
    }
    if !committed {
        if let Some(original_mode) = &journal.original_mode {
            let file = super::open_appimage(&original_mode.path)?;
            file.set_permissions(fs::Permissions::from_mode(original_mode.mode))
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
        }
    }
    remove_if_present(&journal_path(library))?;
    sync_parent(library)
}

pub fn identity(path: &Path) -> Result<Option<(u64, u64)>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            Ok(Some((metadata.dev(), metadata.ino())))
        }
        Ok(_) => Err(format!(
            "recovery conflict at {}: expected a regular file",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn remove_created_artifact(artifact: &Artifact) -> Result<(), String> {
    let original = identity(&artifact.original)?;
    let stage = identity(&artifact.stage)?.or(artifact.identity);
    if original.is_some() && original != stage {
        return Err(format!(
            "recovery conflict at {}; preserve the journal and resolve the foreign file",
            artifact.original.display()
        ));
    }
    if original.is_some() {
        remove_if_present(&artifact.original)?;
    }
    remove_if_present(&artifact.stage)
}

fn restore_removed_artifact(artifact: &Artifact) -> Result<(), String> {
    let stage = identity(&artifact.stage)?;
    let original = identity(&artifact.original)?;
    if stage.is_none() {
        if original.is_none() {
            return Err(format!(
                "recovery cannot find {} or its backup",
                artifact.original.display()
            ));
        }
        return Ok(());
    }
    if original.is_none() {
        fs::hard_link(&artifact.stage, &artifact.original).map_err(|error| {
            format!(
                "recovery could not restore {} without overwriting: {error}",
                artifact.original.display()
            )
        })?;
    } else if original != stage {
        return Err(format!(
            "recovery conflict at {}; original and backup differ",
            artifact.original.display()
        ));
    }
    remove_if_present(&artifact.stage)
}

pub fn remove_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot clean {}: {error}", path.display())),
    }
}

pub fn sync_parent(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or("artifact has no parent")?;
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

fn finish_replacement(journal: &Journal, committed: bool) -> Result<(), String> {
    let [backup, replacement] = journal.artifacts.as_slice() else {
        return Err("invalid replacement journal".into());
    };
    for artifact in [backup, replacement] {
        let stage = identity(&artifact.stage)?;
        if stage.is_some() && stage != artifact.identity {
            return Err("replacement recovery staging file was changed".into());
        }
    }
    if backup.original != replacement.original {
        return finish_relocation(backup, replacement, committed);
    }
    let current = identity(&backup.original)?;
    if current.is_some() && current != backup.identity && current != replacement.identity {
        return Err("replacement recovery destination was changed".into());
    }
    if !committed && current != backup.identity {
        if identity(&backup.stage)? != backup.identity {
            return Err("replacement recovery backup is missing".into());
        }
        fs::rename(&backup.stage, &backup.original).map_err(|error| error.to_string())?;
    }
    if committed && current != replacement.identity {
        return Err("committed replacement is missing".into());
    }
    remove_if_present(&backup.stage)?;
    remove_if_present(&replacement.stage)?;
    sync_parent(&backup.original)
}

fn finish_relocation(
    backup: &Artifact,
    replacement: &Artifact,
    committed: bool,
) -> Result<(), String> {
    let original = identity(&backup.original)?;
    let destination = identity(&replacement.original)?;
    if original.is_some() && original != backup.identity {
        return Err("replacement recovery original was changed".into());
    }
    if destination.is_some() && destination != replacement.identity {
        return Err("replacement recovery destination was changed".into());
    }
    if committed {
        if destination != replacement.identity {
            return Err("committed replacement is missing".into());
        }
        remove_if_present(&backup.original)?;
        remove_if_present(&backup.stage)?;
        remove_if_present(&replacement.stage)?;
    } else {
        restore_removed_artifact(backup)?;
        remove_created_artifact(replacement)?;
    }
    sync_parent(&backup.original)?;
    sync_parent(&replacement.original)
}
