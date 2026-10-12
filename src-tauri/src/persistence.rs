//! Serialized JSON transactions with atomic replacement.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const NO_FOLLOW: i32 = 0x20000;
const NONBLOCKING: i32 = 0x800;

static TRANSACTIONS: Mutex<()> = Mutex::new(());
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn data_path(name: &str) -> Result<PathBuf, String> {
    Ok(dirs::data_dir()
        .ok_or("data directory unavailable")?
        .join("appbento")
        .join(name))
}

pub fn load<T: DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    let _guard = TRANSACTIONS.lock().map_err(|error| error.to_string())?;
    let _file_lock = lock_file(path)?;
    recover_file_batch(
        path.parent()
            .ok_or_else(|| "persistence path has no parent".to_string())?,
    )?;
    read(path)
}

pub(crate) fn read<T: DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid {}: {error}", path.display()))
}

#[derive(Debug)]
pub struct TransactionError {
    pub committed: bool,
    pub message: String,
}

impl From<String> for TransactionError {
    fn from(message: String) -> Self {
        Self {
            committed: false,
            message,
        }
    }
}

pub fn mutate<T, R>(
    path: &Path,
    operation: impl FnOnce(&mut T) -> Result<R, String>,
) -> Result<R, String>
where
    T: DeserializeOwned + Serialize + Default,
{
    mutate_with_commit(path, operation).map_err(|error| error.message)
}

pub struct Repository<T> {
    pub path: PathBuf,
    pub state: T,
}

impl<T: Serialize> Repository<T> {
    pub fn commit(&self) -> Result<(), TransactionError> {
        write_atomic(
            &self.path,
            &serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?,
        )
    }
}

pub fn with_repository<T: DeserializeOwned + Default, R>(
    path: &Path,
    operation: impl FnOnce(&mut Repository<T>) -> Result<R, String>,
) -> Result<R, String> {
    let _guard = TRANSACTIONS.lock().map_err(|error| error.to_string())?;
    let _file_lock = lock_file(path)?;
    recover_file_batch(
        path.parent()
            .ok_or_else(|| "persistence path has no parent".to_string())?,
    )?;
    let mut repository = Repository {
        path: path.to_owned(),
        state: read(path)?,
    };
    operation(&mut repository)
}

pub fn mutate_with_commit<T, R>(
    path: &Path,
    operation: impl FnOnce(&mut T) -> Result<R, String>,
) -> Result<R, TransactionError>
where
    T: DeserializeOwned + Serialize + Default,
{
    let _guard = TRANSACTIONS.lock().map_err(|error| error.to_string())?;
    let _file_lock = lock_file(path)?;
    recover_file_batch(
        path.parent()
            .ok_or_else(|| "persistence path has no parent".to_string())?,
    )?;
    let mut repository = Repository {
        path: path.to_owned(),
        state: read(path)?,
    };
    let result = operation(&mut repository.state)?;
    repository.commit()?;
    Ok(result)
}

/// A durable restore is replayed before any subsequent state read after interruption.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum FileChange {
    Write { path: PathBuf, bytes: Vec<u8> },
    SetPermissions { path: PathBuf, mode: u32 },
}

pub(crate) fn commit_file_batch(parent: &Path, changes: Vec<FileChange>) -> Result<(), String> {
    let journal = parent.join("configuration-restore.json");
    write_atomic(
        &journal,
        &serde_json::to_vec(&changes).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.message)?;
    recover_file_batch(parent)
        .map_err(|error| format!("Restore is pending and will resume on reload: {error}"))
}

fn recover_file_batch(parent: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let journal = parent.join("configuration-restore.json");
    if !journal.exists() {
        return Ok(());
    }
    let changes: Vec<FileChange> = read(&journal)?;
    for change in changes {
        match change {
            FileChange::Write { path, bytes } => {
                write_atomic(&path, &bytes).map_err(|error| error.message)?;
            }
            FileChange::SetPermissions { path, mode } => {
                let file = OpenOptions::new()
                    .read(true)
                    .custom_flags(NO_FOLLOW | NONBLOCKING)
                    .open(path)
                    .map_err(|error| error.to_string())?;
                if !file
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .is_file()
                {
                    return Err("restore target must be a regular file".into());
                }
                file.set_permissions(fs::Permissions::from_mode(mode))
                    .map_err(|error| error.to_string())?;
                file.sync_all().map_err(|error| error.to_string())?;
            }
        }
    }
    fs::remove_file(journal).map_err(|error| error.to_string())?;
    sync_committed_directory(parent).map_err(|error| error.message)
}

fn lock_file(path: &Path) -> Result<fs::File, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "persistence path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let lock_path = parent.join(".appbento-state.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path)
        .map_err(|error| error.to_string())?;
    file.lock().map_err(|error| error.to_string())?;
    Ok(file)
}

pub fn temporary_path(parent: &Path) -> PathBuf {
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".appbento-{}-{sequence}.tmp", std::process::id()))
}

pub fn create_temporary(parent: &Path) -> Result<(PathBuf, fs::File), String> {
    loop {
        let path = temporary_path(parent);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), TransactionError> {
    let parent = path
        .parent()
        .ok_or_else(|| "persistence path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let (temporary, mut file) = create_temporary(parent)?;
    let preparation = (|| {
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if let Err(error) = preparation {
        return match fs::remove_file(&temporary) {
            Ok(()) => Err(error.into()),
            Err(cleanup) => Err(format!("{error}; temporary cleanup failed: {cleanup}").into()),
        };
    }
    sync_committed_directory(parent)
}

fn sync_committed_directory(parent: &Path) -> Result<(), TransactionError> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| TransactionError {
            committed: true,
            message: format!(
                "state committed, but directory durability could not be confirmed: {error}"
            ),
        })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    pub struct TemporaryDirectory(pub PathBuf);

    impl TemporaryDirectory {
        pub fn new() -> Self {
            let path = temporary_path(&std::env::temp_dir());
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove test directory");
        }
    }

    #[test]
    fn durability_failure_is_marked_committed() {
        let directory = TemporaryDirectory::new();
        let file = directory.0.join("not-a-directory");
        let error = sync_committed_directory(&file).unwrap_err();
        assert!(error.committed);
    }

    #[test]
    fn corrupt_data_is_reported_and_preserved() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("library.json");
        fs::write(&path, "not JSON").unwrap();
        assert!(load::<HashMap<String, u64>>(&path).is_err());
        assert!(mutate::<HashMap<String, u64>, ()>(&path, |_| Ok(())).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "not JSON");
    }

    #[test]
    fn concurrent_transactions_preserve_all_changes() {
        let directory = TemporaryDirectory::new();
        let path = Arc::new(directory.0.join("library.json"));
        let threads: Vec<_> = (0..12)
            .map(|index| {
                let path = Arc::clone(&path);
                std::thread::spawn(move || {
                    mutate::<HashMap<String, u64>, ()>(&path, |state| {
                        state.insert(index.to_string(), index);
                        Ok(())
                    })
                    .unwrap()
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(load::<HashMap<String, u64>>(&path).unwrap().len(), 12);
    }

    #[test]
    fn process_transaction_worker() {
        let Some(path) = std::env::var_os("APPBENTO_TEST_REPOSITORY") else {
            return;
        };
        for _ in 0..10 {
            mutate::<HashMap<String, u64>, ()>(&PathBuf::from(&path), |state| {
                *state.entry("count".into()).or_default() += 1;
                Ok(())
            })
            .unwrap();
        }
    }

    #[test]
    fn independent_process_transactions_do_not_lose_updates() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("library.json");
        let mut children: Vec<_> = (0..4)
            .map(|_| {
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "persistence::tests::process_transaction_worker"])
                    .env("APPBENTO_TEST_REPOSITORY", &path)
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .unwrap()
            })
            .collect();
        for child in &mut children {
            assert!(child.wait().unwrap().success());
        }
        assert_eq!(load::<HashMap<String, u64>>(&path).unwrap()["count"], 40);
    }

    #[test]
    fn rejected_transaction_preserves_previous_state() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("library.json");
        mutate::<Vec<String>, ()>(&path, |state| {
            state.push("old".into());
            Ok(())
        })
        .unwrap();
        assert!(mutate::<Vec<String>, ()>(&path, |state| {
            state.clear();
            Err("rejected".into())
        })
        .is_err());
        assert_eq!(load::<Vec<String>>(&path).unwrap(), ["old"]);
    }
}
