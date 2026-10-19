use super::*;
use std::os::unix::fs::MetadataExt;

#[test]
fn scanning_managed_appimage_registers_owned_file_without_changing_it() {
    let (_directory, service, source) = tests::fixture();
    fs::create_dir(&service.managed_dir).unwrap();
    let path = service.managed_dir.join("Example.AppImage");
    fs::rename(source, &path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let before = fs::metadata(&path).unwrap();

    let app = service
        .scan(vec![service.managed_dir.clone()])
        .unwrap()
        .apps
        .remove(0);

    assert!(app.integrated);
    assert!(!app.executable);
    assert_eq!(Path::new(&app.file_path), path);
    assert_eq!(fs::metadata(&path).unwrap().ino(), before.ino());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode(),
        before.permissions().mode()
    );
    assert!(app.source_path.is_none());
    assert_eq!(fs::read_dir(&service.managed_dir).unwrap().count(), 1);
}

#[test]
fn manual_import_from_managed_folder_only_registers_existing_file() {
    for move_to_managed in [false, true] {
        let (_directory, service, source) = tests::fixture();
        fs::create_dir(&service.managed_dir).unwrap();
        let path = service.managed_dir.join("Example.AppImage");
        fs::rename(source, &path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let before = fs::metadata(&path).unwrap();

        let app = service.import_appimage(&path, move_to_managed).unwrap();

        assert_eq!(Path::new(&app.file_path), path);
        assert!(app.integrated);
        assert!(!app.executable);
        assert!(app.source_path.is_none());
        assert_eq!(fs::metadata(&path).unwrap().ino(), before.ino());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode(),
            before.permissions().mode()
        );
        assert_eq!(service.list().unwrap(), vec![app]);
        assert_eq!(fs::read_dir(&service.managed_dir).unwrap().count(), 1);
        assert!(!service.desktop_dir.exists());
    }
}

#[test]
fn external_import_can_stay_in_current_location_without_creating_managed_folder() {
    let (_directory, service, source) = tests::fixture();
    let before = fs::metadata(&source).unwrap();

    let app = service.import_appimage(&source, false).unwrap();

    assert_eq!(Path::new(&app.file_path), source);
    assert!(!app.integrated);
    assert!(!app.executable);
    assert!(!service.managed_dir.exists());
    assert!(app.source_path.is_none());
    assert_eq!(fs::metadata(&source).unwrap().ino(), before.ino());
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode(),
        before.permissions().mode()
    );
    assert_eq!(service.list().unwrap(), vec![app]);
}

#[test]
fn moving_external_import_removes_source_after_commit_and_keeps_filename_and_permissions() {
    for register_first in [false, true] {
        let (directory, service, source) = tests::fixture();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        let bytes = fs::read(&source).unwrap();
        let imported = register_first.then(|| service.import_appimage(&source, false).unwrap());

        let moved = service.import_appimage(&source, true).unwrap();

        assert_eq!(
            Path::new(&moved.file_path),
            service.managed_dir.join("Example.AppImage")
        );
        assert_eq!(moved.file_name, "Example.AppImage");
        assert_eq!(fs::read(&moved.file_path).unwrap(), bytes);
        assert!(!source.exists());
        assert!(moved.integrated);
        assert!(!moved.executable);
        assert!(moved.source_path.is_none());
        assert_eq!(
            fs::metadata(&moved.file_path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        if let Some(imported) = imported {
            assert_eq!(moved.id, imported.id);
            assert_eq!(moved.added_at, imported.added_at);
        }
        service
            .scan(vec![directory.0.clone(), service.managed_dir.clone()])
            .unwrap();
        let rescanned = service.list().unwrap();
        assert_eq!(rescanned.len(), 1);
        assert_eq!(rescanned[0].id, moved.id);
        assert_eq!(rescanned[0].file_path, moved.file_path);
        assert_eq!(fs::read_dir(&service.managed_dir).unwrap().count(), 1);
        assert!(!service
            .library_path
            .with_extension("lifecycle.json")
            .exists());
    }
}

#[test]
fn failed_move_preserves_original_and_library_without_overwriting_destination() {
    let (_directory, service, source) = tests::fixture();
    let imported = service.import_appimage(&source, false).unwrap();
    let bytes = fs::read(&source).unwrap();
    fs::create_dir(&service.managed_dir).unwrap();
    let destination = service.managed_dir.join("Example.AppImage");
    fs::write(&destination, b"existing file").unwrap();

    assert!(service
        .import_appimage(&source, true)
        .unwrap_err()
        .contains("destination already exists"));
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(fs::read(&destination).unwrap(), b"existing file");
    assert_eq!(service.list().unwrap(), vec![imported]);
    assert_eq!(fs::read_dir(&service.managed_dir).unwrap().count(), 1);
}

#[test]
fn imports_reject_invalid_images_symlinks_and_appbento_without_moving_files() {
    for move_to_managed in [false, true] {
        let (directory, service, source) = tests::fixture();
        let link = directory.0.join("Link.AppImage");
        std::os::unix::fs::symlink(&source, &link).unwrap();
        assert!(service.import_appimage(&link, move_to_managed).is_err());
        let appbento = source.with_file_name("AppBento.AppImage");
        fs::copy(&source, &appbento).unwrap();
        assert!(service
            .import_appimage(&appbento, move_to_managed)
            .unwrap_err()
            .contains("cannot add itself"));
        fs::write(&source, b"not an AppImage").unwrap();
        assert!(service.import_appimage(&source, move_to_managed).is_err());
        assert!(source.exists());
        assert!(appbento.exists());
        assert!(!service.managed_dir.exists());
        assert!(service.list().unwrap().is_empty());
    }
}

#[test]
fn moving_a_registered_app_preserves_permissions_history_and_menu_shortcut() {
    let (_directory, service, source) = tests::fixture();
    let app = service.import_appimage(&source, false).unwrap();
    let configured = service
        .configure(
            &app.id,
            &crate::models::LaunchConfig {
                arguments: Vec::new(),
                environment: Default::default(),
                mode: crate::models::LaunchMode::Direct,
                permissions: Default::default(),
            },
        )
        .unwrap();
    let configured = service.set_menu_entry(&configured.id, true).unwrap();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.get_mut(&configured.id).unwrap().launch_count = 4;
        Ok(())
    })
    .unwrap();
    let configured = service.list().unwrap().remove(0);
    let desktop = service
        .desktop_dir
        .join(configured.desktop_id.as_ref().unwrap());
    let desktop_content = fs::read(&desktop).unwrap();

    let moved = service.import_appimage(&source, true).unwrap();

    assert_eq!(moved.id, configured.id);
    assert_eq!(moved.added_at, configured.added_at);
    assert_eq!(moved.launch_count, configured.launch_count);
    assert_eq!(moved.launch_config, configured.launch_config);
    assert_eq!(moved.reviewed_digest, configured.reviewed_digest);
    assert_eq!(moved.desktop_id, configured.desktop_id);
    assert_eq!(fs::read(desktop).unwrap(), desktop_content);
    assert_eq!(service.list().unwrap(), vec![moved]);
}

#[test]
fn interrupted_move_recovers_without_losing_the_source_or_committed_managed_copy() {
    for is_committed in [false, true] {
        let (_directory, service, source) = tests::fixture();
        let app = service.import_appimage(&source, false).unwrap();
        fs::create_dir(&service.managed_dir).unwrap();
        let backup = persistence::temporary_path(source.parent().unwrap());
        fs::hard_link(&source, &backup).unwrap();
        let (stage, file) = persistence::create_temporary(&service.managed_dir).unwrap();
        drop(file);
        fs::copy(&source, &stage).unwrap();
        let destination = service.managed_dir.join("Example.AppImage");
        let moved = InstalledApp {
            file_path: destination.to_str().unwrap().into(),
            integrated: true,
            ..app.clone()
        };
        let journal = recovery::Journal {
            operation: recovery::Operation::Replace,
            app_id: app.id.clone(),
            artifacts: vec![
                recovery::Artifact {
                    original: source.clone(),
                    stage: backup.clone(),
                    identity: recovery::identity(&backup).unwrap(),
                },
                recovery::Artifact {
                    original: destination.clone(),
                    stage: stage.clone(),
                    identity: recovery::identity(&stage).unwrap(),
                },
            ],
            original_mode: None,
            installed_app: Some(moved.clone()),
            previous_library_identity: recovery::identity(&service.library_path).unwrap(),
        };
        recovery::write(&service.library_path, &journal).unwrap();
        fs::hard_link(&stage, &destination).unwrap();
        if is_committed {
            persistence::mutate(&service.library_path, |library: &mut Library| {
                library.insert(moved.id.clone(), moved.clone());
                Ok(())
            })
            .unwrap();
        }

        let apps = service.list().unwrap();

        assert_eq!(source.exists(), !is_committed);
        assert_eq!(destination.exists(), is_committed);
        assert_eq!(apps, vec![if is_committed { moved } else { app }]);
        assert!(!backup.exists());
        assert!(!stage.exists());
        assert!(!service
            .library_path
            .with_extension("lifecycle.json")
            .exists());
    }
}
