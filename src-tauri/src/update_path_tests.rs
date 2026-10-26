use super::*;

const RELEASE_NAME: &str = "Example-2.0.0-x86_64.AppImage";

#[test]
fn unversioned_updates_replace_current_file_through_managed_folder_alias() {
    let (directory, mut service, source) = tests::fixture();
    let alias = directory.0.join("managed-alias");
    fs::create_dir_all(&service.managed_dir).unwrap();
    std::os::unix::fs::symlink(&service.managed_dir, &alias).unwrap();
    service.managed_dir = alias;
    let mut app = service.integrate_download(&source).unwrap();
    let original_path = app.file_path.clone();

    for version in ["2.0.0", "3.0.0"] {
        let digest = crate::sandbox::appimage_digest(Path::new(&app.file_path)).unwrap();
        let staged = tests::staged_update(&directory.0, Path::new(&app.file_path));
        let downloaded = fs::read(&staged).unwrap();

        let updated = service
            .replace_update(&app, &staged, "Example.AppImage", Some(version), &digest)
            .unwrap();

        assert_eq!(updated.file_path, original_path);
        assert_eq!(updated.id, app.id);
        assert_eq!(updated.version_current.as_deref(), Some(version));
        assert_eq!(fs::read(&updated.file_path).unwrap(), downloaded);
        assert_eq!(service.list().unwrap(), vec![updated.clone()]);
        assert_eq!(fs::read_dir(&service.managed_dir).unwrap().count(), 1);
        app = updated;
    }
}

#[test]
fn updates_use_managed_folder_and_official_release_filename() {
    for is_integrated in [false, true] {
        let (directory, service, source) = tests::fixture();
        let app = if is_integrated {
            service.integrate(&source, false).unwrap()
        } else {
            service
                .scan(vec![directory.0.clone()])
                .unwrap()
                .apps
                .remove(0)
        };
        let digest = crate::sandbox::appimage_digest(Path::new(&app.file_path)).unwrap();
        let staged = tests::staged_update(&directory.0, &source);
        let named_download = directory.0.join(RELEASE_NAME);
        fs::rename(staged, &named_download).unwrap();

        let updated = service
            .replace_update(&app, &named_download, RELEASE_NAME, Some("2.0.0"), &digest)
            .unwrap();

        assert_eq!(
            Path::new(&updated.file_path),
            service.managed_dir.join(RELEASE_NAME)
        );
        assert_eq!(updated.file_name, RELEASE_NAME);
        assert_eq!(updated.id, app.id);
        assert_eq!(
            fs::read(&updated.file_path).unwrap(),
            fs::read(&named_download).unwrap()
        );
        assert!(source.exists());
        if is_integrated {
            assert!(!Path::new(&app.file_path).exists());
        }
        fs::remove_file(&named_download).unwrap();
        service
            .scan(vec![directory.0.clone(), service.managed_dir.clone()])
            .unwrap();
        assert_eq!(service.list().unwrap(), vec![updated]);
    }
}

#[test]
fn successive_releases_replace_old_managed_filename_and_preserve_original_source() {
    for is_integrated in [false, true] {
        let (directory, service, source) = tests::fixture();
        let app = if is_integrated {
            service.integrate_download(&source).unwrap()
        } else {
            service
                .scan(vec![directory.0.clone()])
                .unwrap()
                .apps
                .remove(0)
        };
        let digest = crate::sandbox::appimage_digest(Path::new(&app.file_path)).unwrap();
        let staged = tests::staged_update(&directory.0, &source);
        let updated = service
            .replace_update(&app, &staged, RELEASE_NAME, Some("2.0.0"), &digest)
            .unwrap();
        let digest = crate::sandbox::appimage_digest(Path::new(&updated.file_path)).unwrap();
        let next_name = "Example-3.0.0-x86_64.AppImage";
        let staged = tests::staged_update(&directory.0, Path::new(&updated.file_path));
        let next = service
            .replace_update(&updated, &staged, next_name, Some("3.0.0"), &digest)
            .unwrap();

        assert_eq!(
            Path::new(&next.file_path),
            service.managed_dir.join(next_name)
        );
        assert!(!Path::new(&updated.file_path).exists());
        assert_eq!(next.id, app.id);
        assert_eq!(
            next.source_path,
            if is_integrated {
                None
            } else {
                Some(app.file_path)
            }
        );
        assert!(source.exists());
        let scan_directories = if is_integrated {
            vec![service.managed_dir.clone()]
        } else {
            vec![directory.0.clone(), service.managed_dir.clone()]
        };
        service.scan(scan_directories).unwrap();
        assert_eq!(service.list().unwrap(), vec![next]);
        assert_eq!(fs::read_dir(&service.managed_dir).unwrap().count(), 1);
    }
}

#[test]
fn update_refuses_existing_destinations_and_unsafe_release_filenames() {
    let (directory, service, source) = tests::fixture();
    let app = service.integrate(&source, false).unwrap();
    let digest = crate::sandbox::appimage_digest(Path::new(&app.file_path)).unwrap();
    let staged = tests::staged_update(&directory.0, &source);
    let destination = service.managed_dir.join(RELEASE_NAME);
    fs::write(&destination, b"another user's file").unwrap();

    let error = service
        .replace_update(&app, &staged, RELEASE_NAME, Some("2.0.0"), &digest)
        .unwrap_err();
    assert!(error.contains("destination already exists"), "{error}");
    assert_eq!(fs::read(&destination).unwrap(), b"another user's file");

    for name in [
        "../Example.AppImage",
        "/tmp/Example.AppImage",
        "sub/Example.AppImage",
        "sub\\Example.AppImage",
        "Example\n.AppImage",
        "",
        ".",
        "..",
    ] {
        assert!(
            service
                .replace_update(&app, &staged, name, None, &digest)
                .is_err(),
            "{name}"
        );
    }
    fs::remove_file(&destination).unwrap();
    std::os::unix::fs::symlink(directory.0.join("missing"), &destination).unwrap();
    assert!(service
        .replace_update(&app, &staged, RELEASE_NAME, None, &digest)
        .is_err());
    assert!(fs::symlink_metadata(&destination)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!service
        .library_path
        .with_extension("lifecycle.json")
        .exists());
    assert_eq!(service.list().unwrap(), vec![app]);
}

#[test]
fn update_moves_legacy_hashed_download_to_current_managed_folder() {
    let (directory, service, source) = tests::fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let legacy_directory = directory.0.join("updates");
    fs::create_dir(&legacy_directory).unwrap();
    let legacy = legacy_directory.join("0123456789abcdef.AppImage");
    fs::copy(&source, &legacy).unwrap();
    let app = InstalledApp {
        file_path: legacy.to_str().unwrap().into(),
        file_name: legacy.file_name().unwrap().to_str().unwrap().into(),
        source_path: Some(source.to_str().unwrap().into()),
        ..app
    };
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.insert(app.id.clone(), app.clone());
        Ok(())
    })
    .unwrap();
    let digest = crate::sandbox::appimage_digest(&legacy).unwrap();
    let staged = tests::staged_update(&directory.0, &source);

    let updated = service
        .replace_update(&app, &staged, RELEASE_NAME, Some("2.0.0"), &digest)
        .unwrap();

    assert_eq!(
        Path::new(&updated.file_path),
        service.managed_dir.join(RELEASE_NAME)
    );
    assert!(!legacy.exists());
    assert!(source.exists());
    assert_eq!(updated.source_path, app.source_path);
    assert_eq!(service.list().unwrap(), vec![updated]);
}

fn interrupt_renamed_update(service: &Lifecycle, app: &InstalledApp) -> recovery::Journal {
    let current = Path::new(&app.file_path);
    let backup_stage = persistence::temporary_path(current.parent().unwrap());
    fs::hard_link(current, &backup_stage).unwrap();
    let (stage, file) = persistence::create_temporary(&service.managed_dir).unwrap();
    drop(file);
    let mut bytes = fs::read(current).unwrap();
    bytes.extend_from_slice(b"replacement");
    fs::write(&stage, &bytes).unwrap();
    let destination = service.managed_dir.join(RELEASE_NAME);
    let mut updated = app.clone();
    updated.file_path = destination.to_str().unwrap().into();
    updated.file_name = RELEASE_NAME.into();
    updated.size_bytes = bytes.len() as u64;
    updated.version_current = Some("2.0.0".into());
    let journal = recovery::Journal {
        operation: recovery::Operation::Replace,
        app_id: app.id.clone(),
        artifacts: vec![
            recovery::Artifact {
                original: current.to_owned(),
                identity: recovery::identity(&backup_stage).unwrap(),
                stage: backup_stage,
            },
            recovery::Artifact {
                original: destination.clone(),
                identity: recovery::identity(&stage).unwrap(),
                stage: stage.clone(),
            },
        ],
        original_mode: None,
        installed_app: Some(updated),
        previous_library_identity: recovery::identity(&service.library_path).unwrap(),
    };
    recovery::write(&service.library_path, &journal).unwrap();
    fs::hard_link(stage, destination).unwrap();
    journal
}

#[test]
fn renamed_update_recovers_before_and_after_library_commit() {
    for is_committed in [false, true] {
        let (_directory, service, source) = tests::fixture();
        let app = service.integrate_download(&source).unwrap();
        let original = fs::read(&app.file_path).unwrap();
        let journal = interrupt_renamed_update(&service, &app);
        let updated = journal.installed_app.as_ref().unwrap().clone();
        if is_committed {
            persistence::mutate(&service.library_path, |library: &mut Library| {
                library.insert(updated.id.clone(), updated.clone());
                Ok(())
            })
            .unwrap();
        }

        let apps = service.list().unwrap();

        if is_committed {
            assert_eq!(apps, vec![updated.clone()]);
            assert!(!Path::new(&app.file_path).exists());
            assert_ne!(fs::read(&updated.file_path).unwrap(), original);
        } else {
            assert_eq!(apps, vec![app.clone()]);
            assert_eq!(fs::read(&app.file_path).unwrap(), original);
            assert!(!Path::new(&updated.file_path).exists());
        }
        assert!(journal
            .artifacts
            .iter()
            .all(|artifact| !artifact.stage.exists()));
        assert!(!service
            .library_path
            .with_extension("lifecycle.json")
            .exists());
        assert_eq!(service.list().unwrap(), apps);
    }
}

#[test]
fn renamed_update_recovery_preserves_foreign_files() {
    for replace_original in [false, true] {
        let (_directory, service, source) = tests::fixture();
        let app = service.integrate_download(&source).unwrap();
        let journal = interrupt_renamed_update(&service, &app);
        let artifact_index = if replace_original { 0 } else { 1 };
        let destination = &journal.artifacts[artifact_index].original;
        let foreign = service.managed_dir.join("foreign.tmp");
        fs::write(&foreign, b"foreign bytes").unwrap();
        fs::rename(foreign, destination).unwrap();

        assert!(service.list().unwrap_err().contains("was changed"));
        assert_eq!(fs::read(destination).unwrap(), b"foreign bytes");
        assert!(journal
            .artifacts
            .iter()
            .all(|artifact| artifact.stage.exists()));
        assert!(service
            .library_path
            .with_extension("lifecycle.json")
            .exists());
    }
}
