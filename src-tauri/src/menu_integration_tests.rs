use super::*;

#[test]
fn importing_and_discovering_apps_never_creates_launchers() {
    let (directory, service, source) = fixture();
    let discovered = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    assert!(discovered.desktop_id.is_none());
    let imported = service.integrate(&source, false).unwrap();
    assert!(imported.desktop_id.is_none());
    assert!(!service.desktop_dir.exists());
    service.configure(&imported.id, &direct_config()).unwrap();
    assert!(!service.desktop_dir.exists());
    service.remove(&imported.id, true).unwrap();
    assert!(source.exists());
    assert!(!Path::new(&imported.file_path).exists());
}

fn direct_config() -> crate::models::LaunchConfig {
    crate::models::LaunchConfig {
        mode: crate::models::LaunchMode::Direct,
        arguments: Vec::new(),
        environment: Default::default(),
        permissions: Default::default(),
    }
}

#[test]
fn menu_creation_and_removal_preserve_portable_and_managed_apps() {
    for is_imported in [false, true] {
        let (directory, service, source) = fixture();
        let app = if is_imported {
            service.integrate(&source, false).unwrap()
        } else {
            service
                .scan(vec![directory.0.clone()])
                .unwrap()
                .apps
                .remove(0)
        };
        let enabled = service.set_menu_entry(&app.id, true).unwrap();
        let path = service
            .desktop_dir
            .join(enabled.desktop_id.as_ref().unwrap());
        assert!(path.exists());
        assert_eq!(enabled.file_path, app.file_path);
        assert_eq!(enabled.integrated, app.integrated);
        assert_eq!(service.set_menu_entry(&app.id, true).unwrap(), enabled);
        let disabled = service.set_menu_entry(&app.id, false).unwrap();
        assert!(!path.exists());
        assert_eq!(disabled, app);
        assert!(Path::new(&app.file_path).exists());
        assert_eq!(service.set_menu_entry(&app.id, false).unwrap(), app);
        assert_eq!(service.list().unwrap(), vec![app]);
        assert!(!path.exists());
    }
}

#[test]
fn menu_changes_refuse_foreign_entries_and_unknown_apps() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source, false).unwrap();
    fs::create_dir_all(&service.desktop_dir).unwrap();
    let path = service
        .desktop_dir
        .join(desktop_id(Path::new(&app.file_path)));
    fs::write(&path, "foreign launcher").unwrap();
    assert!(service.set_menu_entry(&app.id, true).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "foreign launcher");
    assert!(service.set_menu_entry("unknown", true).is_err());
    fs::remove_file(&path).unwrap();
    service.set_menu_entry(&app.id, true).unwrap();
    fs::write(&path, "foreign launcher").unwrap();
    assert!(service.set_menu_entry(&app.id, false).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "foreign launcher");
    assert!(Lifecycle::list_at(&service.library_path).unwrap()[0]
        .desktop_id
        .is_some());
}

#[test]
fn launch_config_changes_refresh_existing_menu_entries_without_exposing_environment_values() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    let path = service.desktop_dir.join(app.desktop_id.as_ref().unwrap());
    let mut previous = fs::read_to_string(&path).unwrap();
    let mut config = direct_config();
    for change in 0..4 {
        match change {
            0 => config.arguments = vec!["--profile".into(), "My profile".into()],
            1 => {
                config
                    .environment
                    .insert("LABEL".into(), "private value".into());
            }
            2 => config.mode = crate::models::LaunchMode::Bubblewrap,
            _ => config.mode = crate::models::LaunchMode::Direct,
        }
        service.configure(&app.id, &config).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_ne!(content, previous);
        assert!(content.contains(&format!(" --launch \"{}\"", app.id)));
        assert!(!content.contains("private value"));
        assert_eq!(
            service.list().unwrap()[0].launch_config.as_ref(),
            Some(&config)
        );
        previous = content;
    }
}

#[test]
fn removing_library_entry_also_removes_portable_app_shortcut() {
    let (directory, service, source) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let app = service.set_menu_entry(&app.id, true).unwrap();
    let path = service.desktop_dir.join(app.desktop_id.unwrap());
    service.remove(&app.id, false).unwrap();
    assert!(!path.exists());
    assert!(source.exists());
}

#[test]
fn interrupted_menu_changes_recover_according_to_library_commit() {
    for enabled in [false, true] {
        for committed in [false, true] {
            let (_directory, service, source) = fixture();
            let imported = service.integrate(&source, false).unwrap();
            let previous = if enabled {
                imported
            } else {
                service.set_menu_entry(&imported.id, true).unwrap()
            };
            let desktop_id = desktop_id(Path::new(&previous.file_path));
            let path = service.desktop_dir.join(&desktop_id);
            fs::create_dir_all(&service.desktop_dir).unwrap();
            if enabled {
                fs::write(&path, desktop::content_for_entry(&previous).unwrap()).unwrap();
            }
            let stage = path.with_extension("staged");
            fs::hard_link(&path, &stage).unwrap();
            let artifact = recovery::Artifact {
                original: path.clone(),
                stage: stage.clone(),
                identity: recovery::identity(&stage).unwrap(),
            };
            if !enabled {
                fs::remove_file(&path).unwrap();
            }
            let mut expected = previous.clone();
            expected.desktop_id = enabled.then_some(desktop_id);
            let journal = recovery::Journal {
                operation: if enabled {
                    recovery::Operation::MenuCreate
                } else {
                    recovery::Operation::MenuRemove
                },
                app_id: previous.id.clone(),
                artifacts: vec![artifact],
                original_mode: None,
                installed_app: Some(expected.clone()),
                previous_library_identity: recovery::identity(&service.library_path).unwrap(),
            };
            recovery::write(&service.library_path, &journal).unwrap();
            if committed {
                persistence::mutate(&service.library_path, |library: &mut Library| {
                    library.insert(expected.id.clone(), expected.clone());
                    Ok(())
                })
                .unwrap();
            }
            assert_eq!(
                service.list().unwrap(),
                vec![if committed { expected } else { previous }]
            );
            assert_eq!(path.exists(), enabled == committed);
            assert!(!stage.exists());
            assert!(!service
                .library_path
                .with_extension("lifecycle.json")
                .exists());
            assert!(source.exists());
        }
    }
}
