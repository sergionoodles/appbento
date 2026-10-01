use super::*;

#[test]
fn startup_recovers_interrupted_removal_before_commit() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    let app_path = PathBuf::from(&app.file_path);
    let desktop = service.desktop_dir.join(app.desktop_id.as_ref().unwrap());
    let app_stage = app_path.with_extension("removing");
    let desktop_stage = desktop.with_extension("removing");
    fs::rename(&app_path, &app_stage).unwrap();
    fs::rename(&desktop, &desktop_stage).unwrap();
    let journal = serde_json::json!({"operation":"remove", "app_id":app.id,
        "artifacts":[{"original":app_path,"stage":app_stage},{"original":desktop,"stage":desktop_stage}],
        "original_mode":null});
    fs::write(
        service.library_path.with_extension("lifecycle.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    Lifecycle::list_at(&service.library_path).unwrap();
    assert!(app_path.exists());
    assert!(desktop.exists());
}

fn crash_artifact(original: &Path) -> recovery::Artifact {
    let stage = persistence::temporary_path(original.parent().unwrap());
    fs::hard_link(original, &stage).unwrap();
    recovery::Artifact {
        original: original.to_owned(),
        identity: recovery::identity(&stage).unwrap(),
        stage,
    }
}

#[test]
fn startup_rolls_back_uncommitted_integration_and_preserves_source() {
    let (directory, service, source) = fixture();
    let portable = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    fs::create_dir(&service.managed_dir).unwrap();
    fs::create_dir(&service.desktop_dir).unwrap();
    let destination = service.managed_dir.join(source.file_name().unwrap());
    fs::copy(&source, &destination).unwrap();
    let desktop = service.desktop_dir.join(desktop_id(&destination));
    fs::write(
        &desktop,
        desktop_content(&destination, &portable.name).unwrap(),
    )
    .unwrap();
    let artifacts = vec![crash_artifact(&destination), crash_artifact(&desktop)];
    let mut expected = portable.clone();
    expected.id = destination.to_str().unwrap().to_owned();
    expected.file_path = expected.id.clone();
    expected.integrated = true;
    expected.executable = true;
    expected.desktop_id = Some(desktop_id(&destination));
    expected.source_path = Some(portable.id);
    let journal = recovery::Journal {
        operation: recovery::Operation::Integrate,
        app_id: expected.id.clone(),
        artifacts,
        original_mode: None,
        installed_app: Some(expected),
        previous_library_identity: Some(
            recovery::identity(&service.library_path).unwrap().unwrap(),
        ),
    };
    recovery::write(&service.library_path, &journal).unwrap();
    let apps = Lifecycle::list_at(&service.library_path).unwrap();
    assert_eq!(apps.len(), 1);
    assert!(!apps[0].integrated);
    assert!(source.exists());
    assert!(!destination.exists());
    assert!(!desktop.exists());
    assert!(!service
        .library_path
        .with_extension("lifecycle.json")
        .exists());
}

#[test]
fn startup_preserves_committed_integration_and_cleans_stages() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    let desktop = service.desktop_dir.join(app.desktop_id.as_ref().unwrap());
    let artifacts = vec![
        crash_artifact(Path::new(&app.file_path)),
        crash_artifact(&desktop),
    ];
    let journal = recovery::Journal {
        operation: recovery::Operation::Integrate,
        app_id: app.id.clone(),
        artifacts: artifacts.clone(),
        original_mode: None,
        installed_app: Some(app.clone()),
        previous_library_identity: None,
    };
    recovery::write(&service.library_path, &journal).unwrap();
    assert_eq!(
        Lifecycle::list_at(&service.library_path).unwrap(),
        vec![app.clone()]
    );
    assert!(Path::new(&app.file_path).exists());
    assert!(desktop.exists());
    assert!(artifacts.iter().all(|artifact| !artifact.stage.exists()));
}

#[test]
fn startup_cleans_committed_removal_stages() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    let desktop = service.desktop_dir.join(app.desktop_id.as_ref().unwrap());
    let artifacts = vec![
        crash_artifact(Path::new(&app.file_path)),
        crash_artifact(&desktop),
    ];
    for artifact in &artifacts {
        fs::remove_file(&artifact.original).unwrap();
    }
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.remove(&app.id);
        Ok(())
    })
    .unwrap();
    let journal = recovery::Journal {
        operation: recovery::Operation::Remove,
        app_id: app.id,
        artifacts: artifacts.clone(),
        original_mode: None,
        installed_app: None,
        previous_library_identity: None,
    };
    recovery::write(&service.library_path, &journal).unwrap();
    assert!(Lifecycle::list_at(&service.library_path)
        .unwrap()
        .is_empty());
    assert!(artifacts
        .iter()
        .all(|artifact| !artifact.stage.exists() && !artifact.original.exists()));
    assert!(source.exists());
}

#[test]
fn startup_conflicts_do_not_overwrite_foreign_files() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    let artifact = crash_artifact(Path::new(&app.file_path));
    fs::remove_file(&artifact.original).unwrap();
    fs::write(&artifact.original, "foreign file").unwrap();
    let journal = recovery::Journal {
        operation: recovery::Operation::Remove,
        app_id: app.id,
        artifacts: vec![artifact.clone()],
        original_mode: None,
        installed_app: None,
        previous_library_identity: None,
    };
    recovery::write(&service.library_path, &journal).unwrap();
    assert!(Lifecycle::list_at(&service.library_path).is_err());
    assert_eq!(
        fs::read_to_string(&artifact.original).unwrap(),
        "foreign file"
    );
    assert!(artifact.stage.exists());
    assert!(service
        .library_path
        .with_extension("lifecycle.json")
        .exists());
}

#[test]
fn startup_restores_permissions_after_uncommitted_in_place_integration() {
    let (directory, service, source) = fixture();
    let portable = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
    let mut expected = portable.clone();
    expected.integrated = true;
    expected.executable = true;
    let journal = recovery::Journal {
        operation: recovery::Operation::Integrate,
        app_id: expected.id.clone(),
        artifacts: Vec::new(),
        original_mode: Some(recovery::OriginalMode {
            path: source.clone(),
            mode: 0o640,
        }),
        installed_app: Some(expected),
        previous_library_identity: Some(
            recovery::identity(&service.library_path).unwrap().unwrap(),
        ),
    };
    recovery::write(&service.library_path, &journal).unwrap();
    Lifecycle::list_at(&service.library_path).unwrap();
    assert_eq!(
        fs::metadata(source).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn idempotent_integration_restores_owner_execution_and_persists_metadata() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    fs::set_permissions(&app.file_path, fs::Permissions::from_mode(0o645)).unwrap();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.get_mut(&app.id).unwrap().executable = false;
        Ok(())
    })
    .unwrap();
    let refreshed = service.integrate(Path::new(&app.file_path)).unwrap();
    assert!(refreshed.executable);
    assert_eq!(
        fs::metadata(&app.file_path).unwrap().permissions().mode() & 0o777,
        0o745
    );
    assert!(Lifecycle::list_at(&service.library_path).unwrap()[0].executable);
}

#[test]
fn scan_reports_invalid_candidates_without_losing_valid_apps() {
    let (directory, service, source) = fixture();
    let invalid = directory.0.join("Bad.AppImage");
    fs::write(&invalid, "not an AppImage").unwrap();
    let wrong_architecture = directory.0.join("OtherArchitecture.AppImage");
    let mut bytes = fs::read(&source).unwrap();
    let other: u16 = if std::env::consts::ARCH == "aarch64" {
        62
    } else {
        183
    };
    bytes[18..20].copy_from_slice(&other.to_le_bytes());
    fs::write(wrong_architecture, bytes).unwrap();
    let report = service.scan(vec![directory.0.clone()]).unwrap();
    assert_eq!(report.apps.len(), 1);
    assert_eq!(report.warnings.len(), 2);
    assert_eq!(report.apps[0].file_path, source.to_str().unwrap());
}

#[test]
fn startup_distinguishes_uncommitted_integration_with_unchanged_metadata() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    fs::set_permissions(&app.file_path, fs::Permissions::from_mode(0o700)).unwrap();
    let desktop = service.desktop_dir.join(app.desktop_id.as_ref().unwrap());
    fs::remove_file(&desktop).unwrap();
    let stage = persistence::temporary_path(&service.desktop_dir);
    fs::write(
        &stage,
        desktop_content(Path::new(&app.file_path), &app.name).unwrap(),
    )
    .unwrap();
    fs::hard_link(&stage, &desktop).unwrap();
    let journal = serde_json::json!({
        "operation": "integrate", "app_id": app.id,
        "artifacts": [{"original": desktop, "stage": stage, "identity": recovery::identity(&stage).unwrap()}],
        "original_mode": {"path": app.file_path, "mode": 0o640},
        "installed_app": app,
        "previous_library_identity": recovery::identity(&service.library_path).unwrap(),
    });
    fs::write(
        service.library_path.with_extension("lifecycle.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    Lifecycle::list_at(&service.library_path).unwrap();
    assert_eq!(
        fs::metadata(&app.file_path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(!desktop.exists());
    assert!(!stage.exists());
}

fn interrupted_replacement(
    service: &Lifecycle,
    app: &InstalledApp,
) -> (recovery::Journal, Vec<u8>) {
    let destination = Path::new(&app.file_path);
    let original = fs::read(destination).unwrap();
    let backup = crash_artifact(destination);
    let stage = persistence::temporary_path(destination.parent().unwrap());
    let mut updated_bytes = original.clone();
    updated_bytes.extend_from_slice(b"replacement");
    fs::write(&stage, updated_bytes).unwrap();
    let replacement = recovery::Artifact {
        original: destination.to_owned(),
        identity: recovery::identity(&stage).unwrap(),
        stage: stage.clone(),
    };
    let mut updated = app.clone();
    updated.version_current = Some("2.0.0".into());
    updated.size_bytes = fs::metadata(&stage).unwrap().len();
    let journal = recovery::Journal {
        operation: recovery::Operation::Replace,
        app_id: app.id.clone(),
        artifacts: vec![backup, replacement],
        original_mode: None,
        installed_app: Some(updated),
        previous_library_identity: recovery::identity(&service.library_path).unwrap(),
    };
    recovery::write(&service.library_path, &journal).unwrap();
    fs::rename(&stage, destination).unwrap();
    (journal, original)
}

#[test]
fn interrupted_update_rolls_back_bytes_and_library_before_commit() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    let (journal, original) = interrupted_replacement(&service, &app);
    assert_eq!(service.list().unwrap(), vec![app.clone()]);
    assert_eq!(fs::read(&app.file_path).unwrap(), original);
    assert!(journal
        .artifacts
        .iter()
        .all(|artifact| !artifact.stage.exists()));
}

#[test]
fn interrupted_committed_update_preserves_new_bytes_and_cleans_backup() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    let (journal, original) = interrupted_replacement(&service, &app);
    let updated = journal.installed_app.as_ref().unwrap().clone();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.insert(updated.id.clone(), updated.clone());
        Ok(())
    })
    .unwrap();
    assert_eq!(service.list().unwrap(), vec![updated]);
    assert_ne!(fs::read(&app.file_path).unwrap(), original);
    assert!(journal
        .artifacts
        .iter()
        .all(|artifact| !artifact.stage.exists()));
}

#[test]
fn interrupted_update_does_not_overwrite_foreign_destination() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    let (journal, _original) = interrupted_replacement(&service, &app);
    fs::remove_file(&app.file_path).unwrap();
    fs::write(&app.file_path, "foreign bytes").unwrap();
    assert!(service.list().is_err());
    assert_eq!(fs::read_to_string(&app.file_path).unwrap(), "foreign bytes");
    assert!(journal.artifacts[0].stage.exists());
}
