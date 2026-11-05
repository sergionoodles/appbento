use super::desktop_process::{lock_shared, remove_stale_directories};
use crate::persistence::tests::TemporaryDirectory;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

const LIVE_ID: &str = "appbento-00000000000000aa";
const DEAD_ID: &str = "appbento-00000000000000bb";

fn instance(runtime: &Path, id: &str) -> PathBuf {
    runtime.join(".flatpak").join(id)
}

fn proxy(runtime: &Path, id: &str) -> PathBuf {
    runtime.join("appbento/dbus").join(id)
}

/// Creates both directories of one launch, with an unlocked `.ref`.
fn leftover_launch(runtime: &Path, id: &str) -> File {
    fs::create_dir_all(instance(runtime, id)).unwrap();
    fs::create_dir_all(proxy(runtime, id)).unwrap();
    // Readable, like the launch monitor's reference: read locks need read access.
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(instance(runtime, id).join(".ref"))
        .unwrap()
}

#[test]
fn removes_directories_of_launches_that_no_longer_hold_their_lock() {
    let runtime = TemporaryDirectory::new();
    drop(leftover_launch(&runtime.0, DEAD_ID));

    remove_stale_directories(&runtime.0).unwrap();

    assert!(!instance(&runtime.0, DEAD_ID).exists());
    assert!(!proxy(&runtime.0, DEAD_ID).exists());
}

#[test]
fn removes_proxy_directories_whose_instance_is_already_gone() {
    let runtime = TemporaryDirectory::new();
    fs::create_dir_all(proxy(&runtime.0, DEAD_ID)).unwrap();

    remove_stale_directories(&runtime.0).unwrap();

    assert!(!proxy(&runtime.0, DEAD_ID).exists());
}

#[test]
fn keeps_running_and_starting_launches() {
    let runtime = TemporaryDirectory::new();
    let reference = leftover_launch(&runtime.0, LIVE_ID);
    lock_shared(&reference).unwrap();
    let starting = "appbento-00000000000000cc";
    fs::create_dir_all(instance(&runtime.0, starting)).unwrap();

    remove_stale_directories(&runtime.0).unwrap();

    assert!(instance(&runtime.0, LIVE_ID).exists());
    assert!(proxy(&runtime.0, LIVE_ID).exists());
    assert!(instance(&runtime.0, starting).exists());
    drop(reference);
}

#[test]
fn leaves_directories_it_does_not_own_and_tolerates_missing_roots() {
    let runtime = TemporaryDirectory::new();
    remove_stale_directories(&runtime.0).unwrap();

    let others = [
        instance(&runtime.0, "1234567890"),
        instance(&runtime.0, "appbento-not-hex"),
        proxy(&runtime.0, "notes"),
    ];
    for directory in &others {
        fs::create_dir_all(directory).unwrap();
    }
    File::create(instance(&runtime.0, "1234567890").join(".ref")).unwrap();

    remove_stale_directories(&runtime.0).unwrap();

    assert!(others.iter().all(|directory| directory.exists()));
}
