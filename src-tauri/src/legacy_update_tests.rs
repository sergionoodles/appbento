use super::*;

#[test]
fn legacy_update_replaces_its_original_source_in_managed_folder() {
    let (directory, service, source) = tests::fixture();
    fs::create_dir_all(&service.managed_dir).unwrap();
    let original = service.managed_dir.join("Example.AppImage");
    fs::copy(&source, &original).unwrap();
    let app = service.import_appimage(&original, false).unwrap();
    let legacy = directory.0.join("cached.AppImage");
    fs::copy(&source, &legacy).unwrap();
    let app = InstalledApp {
        file_path: legacy.to_str().unwrap().into(),
        file_name: "cached.AppImage".into(),
        source_path: Some(original.to_str().unwrap().into()),
        integrated: false,
        ..app
    };
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.insert(app.id.clone(), app.clone());
        Ok(())
    })
    .unwrap();
    let digest = crate::sandbox::appimage_digest(&legacy).unwrap();
    let staged = tests::staged_update(&directory.0, &legacy);
    let downloaded = fs::read(&staged).unwrap();
    let other = InstalledApp {
        id: "another-app".into(),
        file_path: original.to_str().unwrap().into(),
        ..app.clone()
    };
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.insert(other.id.clone(), other.clone());
        Ok(())
    })
    .unwrap();
    let error = service
        .replace_update(&app, &staged, "Example.AppImage", Some("2.0.0"), &digest)
        .unwrap_err();
    assert!(error.contains("destination already exists"), "{error}");
    assert_eq!(fs::read(&original).unwrap(), fs::read(&source).unwrap());
    assert!(legacy.exists());
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.remove(&other.id);
        Ok(())
    })
    .unwrap();

    let updated = service
        .replace_update(&app, &staged, "Example.AppImage", Some("2.0.0"), &digest)
        .unwrap();

    assert_eq!(Path::new(&updated.file_path), original);
    assert_eq!(updated.id, app.id);
    assert_eq!(fs::read(&original).unwrap(), downloaded);
    assert!(!legacy.exists());
    assert_eq!(service.list().unwrap(), vec![updated]);
    assert_eq!(fs::read_dir(&service.managed_dir).unwrap().count(), 1);
}

#[test]
fn source_replacement_recovers_before_install_before_commit_and_after_commit() {
    for phase in ["prepared", "installed", "committed", "committed-missing"] {
        let (directory, service, source) = tests::fixture();
        let app = service.integrate_download(&source).unwrap();
        let original = Path::new(&app.file_path);
        let original_bytes = fs::read(original).unwrap();
        let legacy = directory.0.join("cached.AppImage");
        fs::copy(original, &legacy).unwrap();
        let cached = InstalledApp {
            file_path: legacy.to_str().unwrap().into(),
            source_path: Some(app.file_path.clone()),
            ..app.clone()
        };
        persistence::mutate(&service.library_path, |library: &mut Library| {
            library.insert(cached.id.clone(), cached.clone());
            Ok(())
        })
        .unwrap();
        let staged = tests::staged_update(&directory.0, &legacy);
        let downloaded = fs::read(&staged).unwrap();
        let updated = InstalledApp {
            size_bytes: downloaded.len() as u64,
            version_current: Some("2.0.0".into()),
            ..app.clone()
        };
        let journal = recovery::Journal {
            operation: recovery::Operation::Replace,
            app_id: updated.id.clone(),
            artifacts: vec![
                replacement::reserve_backup(original).unwrap(),
                recovery::Artifact {
                    original: original.to_owned(),
                    stage: staged.clone(),
                    identity: recovery::identity(&staged).unwrap(),
                },
                replacement::reserve_backup(&legacy).unwrap(),
            ],
            original_mode: None,
            installed_app: Some(updated.clone()),
            previous_library_identity: recovery::identity(&service.library_path).unwrap(),
        };
        recovery::write(&service.library_path, &journal).unwrap();
        if phase != "prepared" {
            fs::rename(&staged, original).unwrap();
        }
        if phase.starts_with("committed") {
            persistence::mutate(&service.library_path, |library: &mut Library| {
                library.insert(updated.id.clone(), updated.clone());
                Ok(())
            })
            .unwrap();
        }
        if phase == "committed-missing" {
            fs::remove_file(original).unwrap();
            assert!(service
                .list()
                .unwrap_err()
                .contains("committed replacement is missing"));
            assert_eq!(fs::read(&legacy).unwrap(), original_bytes);
            continue;
        }

        let recovered = service.list().unwrap();

        if phase == "committed" {
            assert_eq!(recovered, vec![updated]);
            assert_eq!(fs::read(original).unwrap(), downloaded);
            assert!(!legacy.exists());
        } else {
            assert_eq!(recovered, vec![cached]);
            assert_eq!(fs::read(original).unwrap(), original_bytes);
            assert_eq!(fs::read(&legacy).unwrap(), original_bytes);
        }
        for artifact in &journal.artifacts {
            assert!(!artifact.stage.exists());
        }
        assert!(!service
            .library_path
            .with_extension("lifecycle.json")
            .exists());
    }
}
