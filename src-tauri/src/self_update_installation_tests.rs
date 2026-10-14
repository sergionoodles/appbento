use super::*;
use crate::persistence::tests::TemporaryDirectory;
use std::os::unix::fs::PermissionsExt;

fn install_fixture(directory: &Path) -> (PathBuf, TemporaryFile) {
    let current = directory.join("AppBento_0.5.0_amd64.AppImage");
    fs::write(&current, b"old image").unwrap();
    let installed = install_image(
        &current,
        &directory.join("staging.AppImage"),
        "0.6.0",
        |path| {
            fs::write(path, b"new image").map_err(|error| error.to_string())?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .map_err(|error| error.to_string())
        },
    )
    .unwrap();
    (current, installed)
}

#[test]
fn installs_under_new_version_and_removes_old_image_only_after_startup() {
    let directory = TemporaryDirectory::new();
    let (current, installed) = install_fixture(&directory.0);
    let destination = installed.path.clone();
    assert!(destination
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .contains("0.6.0"));
    assert!(!directory.0.join("staging.AppImage").exists());
    activate_image(
        &current,
        installed,
        |_| Ok(None),
        |path| {
            assert_eq!(fs::read(&current).unwrap(), b"old image");
            assert_eq!(fs::read(path).unwrap(), b"new image");
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o755
            );
            Ok(())
        },
        || panic!("successful startup must not roll back library launchers"),
    )
    .unwrap();
    assert!(!current.exists());
    assert_eq!(fs::read(destination).unwrap(), b"new image");
}

#[test]
fn installation_failure_preserves_old_image_and_cleans_staging() {
    let directory = TemporaryDirectory::new();
    let current = directory.0.join("old.AppImage");
    let staging = directory.0.join("staging.AppImage");
    fs::write(&current, b"old image").unwrap();
    assert!(install_image(&current, &staging, "0.6.0", |path| {
        fs::write(path, b"partial image").unwrap();
        Err("invalid update".into())
    })
    .is_err());
    assert!(!staging.exists());
    assert_eq!(fs::read(current).unwrap(), b"old image");
}

#[test]
fn existing_release_file_is_never_overwritten() {
    let directory = TemporaryDirectory::new();
    let current = directory.0.join("old.AppImage");
    let destination = updated_path(&current, "0.6.0").unwrap();
    fs::write(&destination, b"existing file").unwrap();
    assert!(install_image(
        &current,
        &directory.0.join("staging.AppImage"),
        "0.6.0",
        |path| { fs::write(path, b"new image").map_err(|error| error.to_string()) }
    )
    .is_err());
    assert_eq!(fs::read(destination).unwrap(), b"existing file");
}

#[test]
fn launch_failure_preserves_old_image_and_removes_new_image() {
    let directory = TemporaryDirectory::new();
    let (current, installed) = install_fixture(&directory.0);
    let destination = installed.path.clone();
    assert!(activate_image(
        &current,
        installed,
        |_| Ok(None),
        |_| Err("startup failed".into()),
        || Ok(()),
    )
    .is_err());
    assert!(current.exists());
    assert!(!destination.exists());
}

#[test]
fn launcher_failure_preserves_old_image_and_removes_new_image() {
    let directory = TemporaryDirectory::new();
    let (current, installed) = install_fixture(&directory.0);
    let destination = installed.path.clone();
    assert!(activate_image(
        &current,
        installed,
        |_| Err("launcher is read-only".into()),
        |_| {
            panic!("must not launch without updating the launcher");
        },
        || Ok(()),
    )
    .is_err());
    assert!(current.exists());
    assert!(!destination.exists());
}

#[test]
fn failed_library_rollback_retains_both_images_for_recovery() {
    let directory = TemporaryDirectory::new();
    let (current, installed) = install_fixture(&directory.0);
    let destination = installed.path.clone();
    let error = activate_image(
        &current,
        installed,
        |_| Ok(None),
        |_| Err("startup failed".into()),
        || Err("library launcher is read-only".into()),
    )
    .unwrap_err();
    assert!(error.contains("Could not restore library launchers"));
    assert!(current.exists());
    assert!(destination.exists());
}

#[test]
fn failed_startup_restores_library_launchers_before_removing_new_image() {
    let directory = TemporaryDirectory::new();
    let (current, installed) = install_fixture(&directory.0);
    let destination = installed.path.clone();
    let entry = directory.0.join("library.desktop");
    fs::write(&entry, current.to_str().unwrap()).unwrap();
    assert!(activate_image(
        &current,
        installed,
        |_| Ok(None),
        |path| {
            fs::write(&entry, path.to_str().unwrap()).unwrap();
            Err("startup failed after updating library launchers".into())
        },
        || {
            assert!(destination.exists());
            fs::write(&entry, current.to_str().unwrap()).map_err(|error| error.to_string())
        },
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(entry).unwrap(),
        current.to_str().unwrap()
    );
    assert!(current.exists());
    assert!(!destination.exists());
}

#[test]
fn rejects_unsafe_versions_and_replacing_running_path() {
    let current = Path::new("/tmp/old.AppImage");
    for version in ["", "../new", "0.6.0/new", "0.6.0\n"] {
        assert!(updated_path(current, version).is_err());
    }
    let destination = updated_path(current, "0.6.0").unwrap();
    assert!(updated_path(&destination, "0.6.0").is_err());
}

#[test]
fn an_executable_that_exits_does_not_count_as_successful_startup() {
    let directory = TemporaryDirectory::new();
    let ready = directory.0.join("startup.ready");
    assert!(launch_and_wait(Path::new("/bin/false"), &ready).is_err());
    assert!(!ready.exists());
}

#[test]
fn waits_for_startup_confirmation_from_the_new_process() {
    let directory = TemporaryDirectory::new();
    let executable = directory.0.join("new.AppImage");
    fs::write(
        &executable,
        b"#!/bin/sh\nprintf ready > \"$APPBENTO_SELF_UPDATE_READY\"\nsleep 1\n",
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let ready = directory.0.join("startup.ready");
    launch_and_wait(&executable, &ready).unwrap();
    assert!(!ready.exists());
}
