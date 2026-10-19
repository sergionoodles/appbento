use super::*;
use crate::persistence::tests::TemporaryDirectory;

#[path = "menu_integration_tests.rs"]
mod menu_integration_tests;

const ELF_HEADER_LENGTH: usize = 20;

#[test]
fn scan_skips_appbento_in_every_scan_directory() {
    let (directory, service, source) = fixture();
    let downloads = directory.0.join("Downloads");
    let extra = directory.0.join("extra");
    for folder in [&downloads, &extra, &service.managed_dir] {
        fs::create_dir_all(folder).unwrap();
        fs::copy(&source, folder.join("AppBento_0.5.3_amd64.AppImage")).unwrap();
    }

    let report = service
        .scan(vec![
            downloads,
            extra,
            service.managed_dir.clone(),
            directory.0.clone(),
        ])
        .unwrap();

    assert_eq!(report.apps.len(), 1);
    assert_eq!(report.apps[0].name, "Example");
    assert!(report.warnings.is_empty());
    assert_eq!(service.list().unwrap().len(), 1);
}

#[test]
fn rescan_removes_a_previously_registered_appbento_without_deleting_its_file() {
    let (directory, service, source) = fixture();
    service.scan(vec![directory.0.clone()]).unwrap();
    let appbento = source.with_file_name("AppBento.AppImage");
    fs::rename(&source, &appbento).unwrap();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        let mut app = library.remove(source.to_str().unwrap()).unwrap();
        app.file_path = appbento.to_str().unwrap().into();
        library.insert(app.id.clone(), app);
        Ok(())
    })
    .unwrap();

    let report = service.scan(vec![directory.0.clone()]).unwrap();

    assert!(report.apps.is_empty());
    assert!(service.list().unwrap().is_empty());
    assert!(appbento.exists());
}

#[test]
fn manual_and_download_imports_reject_appbento_without_creating_artifacts() {
    let (_directory, service, source) = fixture();
    let appbento = source.with_file_name("appbento-0.5.3-x86_64.appimage");
    fs::rename(&source, &appbento).unwrap();

    for result in [
        service.integrate(&appbento, false),
        service.integrate_download(&appbento),
    ] {
        let warning = result.unwrap_err();
        assert!(warning.contains("AppBento cannot add itself"), "{warning}");
        assert!(warning.contains("already running"), "{warning}");
    }
    assert!(appbento.exists());
    assert!(!service.managed_dir.exists());
    assert!(!service.desktop_dir.exists());
    assert!(service.list().unwrap().is_empty());
}

pub(super) fn fixture() -> (TemporaryDirectory, Lifecycle, PathBuf) {
    let directory = TemporaryDirectory::new();
    let service = Lifecycle {
        library_path: directory.0.join("library.json"),
        managed_dir: directory.0.join("managed"),
        desktop_dir: directory.0.join("desktop"),
    };
    let source = directory.0.join("Example.AppImage");
    let mut header = [0; ELF_HEADER_LENGTH];
    header[..4].copy_from_slice(b"\x7fELF");
    header[4] = 2;
    header[5] = 1;
    header[8..11].copy_from_slice(b"AI\x02");
    let architecture: u16 = if std::env::consts::ARCH == "aarch64" {
        183
    } else {
        62
    };
    header[18..20].copy_from_slice(&architecture.to_le_bytes());
    fs::write(&source, header).unwrap();
    (directory, service, source)
}

#[test]
fn missing_appimage_reports_its_path_and_recovery_action() {
    let (_directory, _service, source) = fixture();
    fs::remove_file(&source).unwrap();

    let error = open_appimage(&source).unwrap_err();

    assert!(error.contains(source.to_str().unwrap()), "{error}");
    assert!(error.contains("scan"), "{error}");
    assert!(error.contains("remove the old library entry"), "{error}");
}

#[test]
fn scan_preserves_history_and_desktop_metadata() {
    let (directory, service, source) = fixture();
    service.scan(vec![directory.0.clone()]).unwrap();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        let app = library.get_mut(source.to_str().unwrap()).unwrap();
        app.launch_count = 12;
        app.last_launched = Some(Utc::now());
        app.desktop_id = Some("preserve.desktop".into());
        Ok(())
    })
    .unwrap();
    let before: Library = persistence::load(&service.library_path).unwrap();
    let result = service.scan(vec![directory.0.clone()]).unwrap();
    let previous = before.get(source.to_str().unwrap()).unwrap();
    assert_eq!(result.apps[0].launch_count, 12);
    assert_eq!(result.apps[0].last_launched, previous.last_launched);
    assert_eq!(result.apps[0].added_at, previous.added_at);
    assert_eq!(result.apps[0].desktop_id, previous.desktop_id);
}

#[test]
fn scan_records_when_the_file_was_created() {
    let (directory, service, _source) = fixture();
    let before = Utc::now() - chrono::Duration::minutes(1);
    let result = service.scan(vec![directory.0.clone()]).unwrap();
    let created = result.apps[0]
        .file_created_at
        .expect("creation time recorded");
    assert!(created > before && created <= Utc::now());
}

#[test]
fn integration_preserves_source_and_history_without_duplicate_scan_records() {
    let (directory, service, source) = fixture();
    service.scan(vec![directory.0.clone()]).unwrap();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library
            .get_mut(source.to_str().unwrap())
            .unwrap()
            .launch_count = 4;
        Ok(())
    })
    .unwrap();
    let app = service.integrate(&source, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    assert!(source.exists());
    assert_eq!(app.launch_count, 4);
    assert!(service.desktop_dir.join(app.desktop_id.unwrap()).exists());
    service
        .scan(vec![directory.0.clone(), service.managed_dir.clone()])
        .unwrap();
    assert_eq!(
        persistence::load::<Library>(&service.library_path)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn integration_does_not_overwrite_existing_destination() {
    let (_directory, service, source) = fixture();
    fs::create_dir(&service.managed_dir).unwrap();
    let destination = service.managed_dir.join(source.file_name().unwrap());
    fs::write(&destination, "existing contents").unwrap();
    assert!(service.integrate(&source, false).is_err());
    assert_eq!(
        fs::read_to_string(destination).unwrap(),
        "existing contents"
    );
}

#[test]
fn failed_integration_rolls_back_created_files() {
    let (_directory, service, source) = fixture();
    fs::create_dir(&service.desktop_dir).unwrap();
    let destination = service.managed_dir.join(source.file_name().unwrap());
    let desktop = service.desktop_dir.join(desktop_id(&destination));
    fs::write(&desktop, "foreign launcher").unwrap();
    let app = service.integrate(&source, false).unwrap();
    assert!(service.set_menu_entry(&app.id, true).is_err());
    assert!(destination.exists());
    assert!(service.list().unwrap()[0].desktop_id.is_none());
    assert_eq!(fs::read_to_string(desktop).unwrap(), "foreign launcher");
    assert!(source.exists());
}

#[test]
fn remove_owned_app_cleans_desktop_and_preserves_original() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    service.remove(&app.id, true).unwrap();
    assert!(!Path::new(&app.file_path).exists());
    assert!(!service.desktop_dir.join(app.desktop_id.unwrap()).exists());
    assert!(source.exists());
    assert!(persistence::load::<Library>(&service.library_path)
        .unwrap()
        .is_empty());
}

#[test]
fn rejects_non_appimage_symlink_and_architecture_mismatch() {
    let (directory, _service, source) = fixture();
    let link = directory.0.join("Link.AppImage");
    std::os::unix::fs::symlink(&source, &link).unwrap();
    assert!(open_appimage(&link).is_err());
    let mut header = fs::read(&source).unwrap();
    let wrong_architecture: u16 = if std::env::consts::ARCH == "aarch64" {
        62
    } else {
        183
    };
    header[18..20].copy_from_slice(&wrong_architecture.to_le_bytes());
    fs::write(&source, header).unwrap();
    assert!(open_appimage(&source).is_err());
    fs::write(&source, "arbitrary executable").unwrap();
    assert!(open_appimage(&source).is_err());
}

#[test]
fn desktop_escapes_special_characters_and_uses_stable_unique_ids() {
    let path = Path::new("/tmp/App \"special\" 100%.AppImage");
    let content = desktop_content(path, "name\nExec=bad").unwrap();
    assert!(content.contains("100%%"));
    assert!(content.contains("\\\\\"special\\\\\""));
    assert!(!content.contains("\nExec=bad"));
    assert_ne!(
        desktop_id(Path::new("/a/A.AppImage")),
        desktop_id(Path::new("/a/B.AppImage"))
    );
}

#[test]
fn integrates_nonexecutable_file_already_in_managed_directory() {
    let (_directory, service, source) = fixture();
    fs::create_dir(&service.managed_dir).unwrap();
    let destination = service.managed_dir.join(source.file_name().unwrap());
    fs::rename(&source, &destination).unwrap();
    let app = service.integrate(&destination, false).unwrap();
    assert!(app.integrated);
    assert_ne!(
        fs::metadata(destination).unwrap().permissions().mode() & 0o100,
        0
    );
}

#[test]
fn removing_portable_record_does_not_require_managed_directory() {
    let (directory, service, source) = fixture();
    service.scan(vec![directory.0.clone()]).unwrap();
    service.remove(source.to_str().unwrap(), false).unwrap();
    assert!(source.exists());
}

#[test]
fn rejects_deleting_portable_files() {
    let (directory, service, source) = fixture();
    service.scan(vec![directory.0.clone()]).unwrap();
    assert!(service.remove(source.to_str().unwrap(), true).is_err());
    assert!(source.exists());
}

#[test]
fn integration_uses_generic_desktop_metadata() {
    let (_directory, service, source) = fixture();
    let renamed = source.with_file_name("Neovim-0.10.0.AppImage");
    fs::rename(&source, &renamed).unwrap();
    let app = service.integrate(&renamed, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    assert_eq!(app.name, "Neovim");
    assert_eq!(app.version_current.as_deref(), Some("0.10.0"));
    assert!(app.update_config.is_none());
    let desktop = fs::read_to_string(service.desktop_dir.join(app.desktop_id.unwrap())).unwrap();
    assert!(desktop.contains("Terminal=false\n"));
    assert!(desktop.contains("Categories=Utility;\n"));
    assert!(!desktop.contains("MimeType="));
    service.remove(&app.id, false).unwrap();
}

#[test]
fn removal_after_managed_folder_change_uses_stored_ownership() {
    let (directory, mut service, source) = fixture();
    let app = service.integrate(&source, false).unwrap();
    service.managed_dir = directory.0.join("new-missing-folder");
    service.remove(&app.id, true).unwrap();
    assert!(!Path::new(&app.file_path).exists());
}

#[test]
fn integration_cleanup_does_not_delete_appimage_with_temporary_extension() {
    let (_directory, service, source) = fixture();
    let renamed = source.with_extension("tmp");
    fs::rename(source, &renamed).unwrap();
    let app = service.integrate(&renamed, false).unwrap();
    assert!(Path::new(&app.file_path).exists());
}

#[path = "lifecycle_recovery_tests.rs"]
mod recovery_tests;

#[test]
fn replacing_reviewed_appimage_requires_new_review() {
    let (directory, service, source) = fixture();
    service.scan(vec![directory.0.clone()]).unwrap();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        let app = library.get_mut(source.to_str().unwrap()).unwrap();
        app.reviewed_digest = Some(crate::sandbox::appimage_digest(&source)?);
        app.launch_config = Some(crate::models::LaunchConfig {
            arguments: Vec::new(),
            environment: Default::default(),
            mode: crate::models::LaunchMode::Direct,
            permissions: Default::default(),
        });
        Ok(())
    })
    .unwrap();
    let mut bytes = fs::read(&source).unwrap();
    bytes.extend_from_slice(b"changed executable");
    fs::write(&source, bytes).unwrap();
    let apps = service.scan(vec![directory.0.clone()]).unwrap().apps;
    assert!(apps[0].launch_config.is_some());
    assert!(apps[0].reviewed_digest.is_none());
}

#[test]
fn legacy_owned_desktop_entry_is_migrated_before_listing_and_foreign_entry_is_preserved() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    let desktop_path = service.desktop_dir.join(app.desktop_id.as_ref().unwrap());
    let legacy = desktop::legacy_desktop_content(Path::new(&app.file_path), &app.name);
    fs::write(&desktop_path, legacy).unwrap();
    service.list().unwrap();
    let content = fs::read_to_string(&desktop_path).unwrap();
    assert!(content.contains(" --launch "));
    assert!(content.contains("X-AppBento-Managed=true"));
    fs::write(&desktop_path, "foreign entry").unwrap();
    assert!(service.list().is_err());
    assert_eq!(fs::read_to_string(&desktop_path).unwrap(), "foreign entry");
}

#[test]
fn owned_desktop_entry_with_previous_launcher_is_migrated_before_listing() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    let desktop_path = service.desktop_dir.join(app.desktop_id.as_ref().unwrap());
    let current = fs::read_to_string(&desktop_path).unwrap();
    for launcher in ["/previous build/appbento", r#"/previous/$`"%\appbento"#] {
        let legacy_launcher = desktop::legacy_desktop_content(Path::new(launcher), &app.name);
        let previous_exec = legacy_launcher
            .lines()
            .find(|line| line.starts_with("Exec="))
            .unwrap()
            .strip_suffix(" %U")
            .unwrap();
        let previous = current
            .lines()
            .map(|line| {
                if let Some((_, arguments)) = line.split_once(" --launch ") {
                    format!("{previous_exec} --launch {arguments}")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(&desktop_path, previous).unwrap();

        assert_eq!(service.list().unwrap(), vec![app.clone()]);
        assert_eq!(fs::read_to_string(&desktop_path).unwrap(), current);
    }
}

#[test]
fn desktop_migration_preserves_entries_with_invalid_previous_launch_commands() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    let desktop_path = service.desktop_dir.join(app.desktop_id.as_ref().unwrap());
    let invalid_commands = [
        "\"/previous/appbento\" --launch \"another-app\" %U".to_owned(),
        format!("\"relative/appbento\" --launch \"{}\" %U", app.id),
        format!("/previous/appbento --launch \"{}\" %U", app.id),
        format!("\"/previous/\"appbento\" --launch \"{}\" %U", app.id),
        format!("\"/previous/$appbento\" --launch \"{}\" %U", app.id),
        format!("\"/previous/%f\" --launch \"{}\" %U", app.id),
    ];
    for command in invalid_commands {
        let content = format!("[Desktop Entry]\nExec={command}\nX-AppBento-Managed=true\n");
        fs::write(&desktop_path, &content).unwrap();

        assert_eq!(
            service.list().unwrap_err(),
            "desktop entry ownership cannot be verified"
        );
        assert_eq!(fs::read_to_string(&desktop_path).unwrap(), content);
    }
}

#[test]
fn rescan_replaces_filename_defaults_and_renames_the_desktop_entry() {
    let (_directory, service, source) = fixture();
    let renamed = source.with_file_name("Tool-linux-2.1.0-x86_64.AppImage");
    fs::rename(&source, &renamed).unwrap();
    let app = service.integrate(&renamed, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    let managed_dir = Path::new(&app.file_path).parent().unwrap().to_owned();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        let stored = library.get_mut(&app.id).unwrap();
        stored.name = "Tool linux 2.1.0 x86 64".into();
        stored.version_current = None;
        Ok(())
    })
    .unwrap();
    let desktop_path = service.desktop_dir.join(app.desktop_id.as_ref().unwrap());
    let original = fs::read_to_string(&desktop_path).unwrap();
    fs::write(
        &desktop_path,
        original.replace("\nName=Tool\n", "\nName=Tool linux 2.1.0 x86 64\n"),
    )
    .unwrap();

    let rescanned = service.scan(vec![managed_dir]).unwrap().apps.remove(0);
    service.list().unwrap();

    assert_eq!(rescanned.name, "Tool");
    assert_eq!(rescanned.version_current.as_deref(), Some("2.1.0"));
    let desktop = fs::read_to_string(&desktop_path).unwrap();
    assert!(desktop.contains("\nName=Tool\n"), "{desktop}");
}

#[test]
fn rescan_keeps_names_and_versions_that_differ_from_filename_defaults() {
    let (directory, service, source) = fixture();
    let renamed = source.with_file_name("Tool-2.1.0.AppImage");
    fs::rename(&source, &renamed).unwrap();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    persistence::mutate(&service.library_path, |library: &mut Library| {
        let stored = library.get_mut(&app.id).unwrap();
        stored.name = "My Tool".into();
        stored.version_current = Some("3.0.0".into());
        Ok(())
    })
    .unwrap();

    let rescanned = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);

    assert_eq!(rescanned.name, "My Tool");
    assert_eq!(rescanned.version_current.as_deref(), Some("3.0.0"));
}

#[test]
fn packaged_launcher_uses_stable_outer_appimage_path() {
    let directory = TemporaryDirectory::new();
    let stable = directory.0.join("AppBento.AppImage");
    fs::write(&stable, "fixture").unwrap();
    let ephemeral = Path::new("/tmp/.mount_AppBento/usr/bin/appbento");
    assert_eq!(
        desktop::resolve_launcher(ephemeral, Some(&stable)).unwrap(),
        stable
    );
    assert!(desktop::resolve_launcher(ephemeral, Some(Path::new("relative"))).is_err());
}

#[test]
fn reviewed_configuration_persists_and_failed_validation_keeps_it() {
    let (directory, service, source) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let config = crate::models::LaunchConfig {
        arguments: vec!["--profile".into(), "My profile".into()],
        environment: [("LABEL".into(), "literal value".into())].into(),
        mode: crate::models::LaunchMode::Direct,
        permissions: Default::default(),
    };
    let configured = service.configure(&app.id, &config).unwrap();
    assert_eq!(configured.launch_config, Some(config.clone()));
    assert!(configured.reviewed_digest.is_some());
    assert_eq!(service.list().unwrap()[0], configured);
    let invalid = crate::models::LaunchConfig {
        permissions: crate::models::SandboxPermissions {
            paths: vec![crate::models::PathPermission {
                path: "/".into(),
                access: crate::models::PathAccess::ReadWrite,
            }],
            ..Default::default()
        },
        ..config
    };
    assert!(service.configure(&app.id, &invalid).is_err());
    assert_eq!(service.list().unwrap()[0], configured);
    assert!(source.exists());
}

#[test]
fn imports_require_review_without_app_specific_presets() {
    let (directory, service, source) = fixture();
    let matched = directory.0.join("Obsidian-1.2.3.AppImage");
    fs::rename(&source, &matched).unwrap();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    assert_eq!(app.name, "Obsidian");
    assert_eq!(app.version_current.as_deref(), Some("1.2.3"));
    assert!(app.update_config.is_none());
    assert!(app.launch_config.is_none());
    assert!(app.reviewed_digest.is_none());
}

pub(super) fn staged_update(directory: &Path, source: &Path) -> PathBuf {
    let staged = directory.join("download.tmp");
    let mut bytes = fs::read(source).unwrap();
    bytes.extend_from_slice(b"new release");
    fs::write(&staged, bytes).unwrap();
    staged
}

#[test]
fn manual_update_preserves_identity_permissions_history_and_desktop() {
    let (directory, service, source) = fixture();
    let app = service.integrate(&source, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    let config = crate::models::LaunchConfig {
        arguments: Vec::new(),
        environment: Default::default(),
        mode: crate::models::LaunchMode::Direct,
        permissions: Default::default(),
    };
    let configured = service.configure(&app.id, &config).unwrap();
    let digest = crate::sandbox::appimage_digest(Path::new(&app.file_path)).unwrap();
    let staged = staged_update(&directory.0, &source);
    let updated = service
        .replace_update(
            &configured,
            &staged,
            "Example.AppImage",
            Some("2.0.0"),
            &digest,
        )
        .unwrap();
    assert_eq!(updated.id, app.id);
    assert_eq!(updated.file_path, app.file_path);
    assert_eq!(updated.launch_config, Some(config));
    assert_eq!(updated.added_at, app.added_at);
    assert_eq!(updated.desktop_id, app.desktop_id);
    assert_ne!(updated.reviewed_digest, configured.reviewed_digest);
    assert_eq!(updated.version_current.as_deref(), Some("2.0.0"));
    assert!(source.exists());
    assert_eq!(service.list().unwrap(), vec![updated]);
}

#[test]
fn portable_updates_keep_original_and_do_not_duplicate_scan_records() {
    let (directory, service, source) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let digest = crate::sandbox::appimage_digest(&source).unwrap();
    let staged = staged_update(&directory.0, &source);
    let updated = service
        .replace_update(&app, &staged, "Example.AppImage", Some("2.0.0"), &digest)
        .unwrap();
    assert_eq!(updated.id, app.id);
    assert_ne!(updated.file_path, app.file_path);
    assert!(updated.integrated);
    assert_eq!(fs::read(&source).unwrap().len(), ELF_HEADER_LENGTH);
    let internal = Path::new(&updated.file_path).parent().unwrap().to_owned();
    service.scan(vec![directory.0.clone(), internal]).unwrap();
    assert_eq!(service.list().unwrap(), vec![updated.clone()]);
    let next_digest = crate::sandbox::appimage_digest(Path::new(&updated.file_path)).unwrap();
    let next = service
        .replace_update(
            &updated,
            &staged,
            "Example.AppImage",
            Some("3.0.0"),
            &next_digest,
        )
        .unwrap();
    assert_eq!(next.file_path, updated.file_path);
}

#[test]
fn update_rejects_invalid_download_changed_config_and_changed_file() {
    let (directory, service, source) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let digest = crate::sandbox::appimage_digest(&source).unwrap();
    let staged = directory.0.join("invalid.tmp");
    fs::write(&staged, "not AppImage").unwrap();
    assert!(service
        .replace_update(&app, &staged, "Example.AppImage", None, &digest)
        .is_err());
    let staged = staged_update(&directory.0, &source);
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.get_mut(&app.id).unwrap().update_config =
            Some(crate::models::UpdateConfig::DirectUrl {
                url: "https://example.com/app.AppImage".into(),
            });
        Ok(())
    })
    .unwrap();
    assert!(service
        .replace_update(&app, &staged, "Example.AppImage", None, &digest)
        .is_err());
    let current = service.list().unwrap().remove(0);
    let mut bytes = fs::read(&source).unwrap();
    bytes[11] = 42;
    fs::write(&source, bytes).unwrap();
    assert!(service
        .replace_update(&current, &staged, "Example.AppImage", None, &digest)
        .is_err());
    assert_eq!(service.list().unwrap(), vec![current]);
}

#[test]
fn old_library_records_keep_saved_settings_and_discard_removed_metadata() {
    let (directory, service, _source) = fixture();
    let imported = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let config = crate::models::LaunchConfig {
        arguments: Vec::new(),
        environment: Default::default(),
        mode: crate::models::LaunchMode::Direct,
        permissions: crate::models::SandboxPermissions::default(),
    };
    let configured = service.configure(&imported.id, &config).unwrap();
    let mut record = serde_json::to_value(&configured).unwrap();
    record["update_config"] =
        serde_json::json!({ "strategy": "direct-url", "url": "https://example.com/app.AppImage" });
    record["catalog_id"] = serde_json::json!("obsidian");
    record["permission_defaults"] = serde_json::json!({ "network": true });
    let library = serde_json::json!({ configured.id.clone(): record });
    fs::write(&service.library_path, serde_json::to_vec(&library).unwrap()).unwrap();
    let rescanned = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    assert_eq!(rescanned.launch_config, Some(config));
    assert_eq!(
        rescanned.update_config,
        Some(crate::models::UpdateConfig::DirectUrl {
            url: "https://example.com/app.AppImage".into()
        })
    );
    assert_eq!(rescanned.name, configured.name);
    let saved = serde_json::to_value(&rescanned).unwrap();
    assert!(saved.get("catalog_id").is_none());
    assert!(saved.get("permission_defaults").is_none());
}

#[test]
fn update_inherits_permissions_and_history_changed_during_download() {
    let (directory, service, source) = fixture();
    let expected = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let digest = crate::sandbox::appimage_digest(&source).unwrap();
    let staged = staged_update(&directory.0, &source);
    let config = crate::models::LaunchConfig {
        arguments: Vec::new(),
        environment: Default::default(),
        mode: crate::models::LaunchMode::Direct,
        permissions: crate::models::SandboxPermissions {
            network: true,
            ..Default::default()
        },
    };
    service.configure(&expected.id, &config).unwrap();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.get_mut(&expected.id).unwrap().launch_count = 7;
        Ok(())
    })
    .unwrap();
    let updated = service
        .replace_update(
            &expected,
            &staged,
            "Example.AppImage",
            Some("2.0.0"),
            &digest,
        )
        .unwrap();
    assert_eq!(updated.launch_config, Some(config));
    assert_eq!(updated.launch_count, 7);
    assert_eq!(
        updated.reviewed_digest,
        Some(crate::sandbox::appimage_digest(Path::new(&updated.file_path)).unwrap())
    );
}

#[test]
fn integrating_updated_portable_preserves_identity_configuration_and_private_home() {
    let (directory, service, source) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let config = crate::models::LaunchConfig {
        arguments: Vec::new(),
        environment: Default::default(),
        mode: crate::models::LaunchMode::Direct,
        permissions: Default::default(),
    };
    let configured = service.configure(&app.id, &config).unwrap();
    let digest = crate::sandbox::appimage_digest(&source).unwrap();
    let staged = staged_update(&directory.0, &source);
    let updated = service
        .replace_update(
            &configured,
            &staged,
            "Example.AppImage",
            Some("2.0.0"),
            &digest,
        )
        .unwrap();
    let internal = Path::new(&updated.file_path).parent().unwrap().to_owned();
    let integrated = service
        .integrate(Path::new(&updated.file_path), false)
        .unwrap();
    let integrated = service.set_menu_entry(&integrated.id, true).unwrap();
    assert_eq!(integrated.id, updated.id);
    assert_eq!(integrated.launch_config, updated.launch_config);
    assert_eq!(integrated.update_config, updated.update_config);
    assert_eq!(integrated.reviewed_digest, updated.reviewed_digest);
    let desktop = fs::read_to_string(
        service
            .desktop_dir
            .join(integrated.desktop_id.as_ref().unwrap()),
    )
    .unwrap();
    assert!(desktop.contains(&format!(" --launch \"{}\"", integrated.id)));
    service
        .scan(vec![
            directory.0.clone(),
            internal,
            service.managed_dir.clone(),
        ])
        .unwrap();
    assert_eq!(service.list().unwrap().len(), 1);
    assert_eq!(service.list().unwrap()[0].id, updated.id);
    service.remove(&integrated.id, true).unwrap();
    assert!(source.exists());
    assert!(service.list().unwrap().is_empty());
}

#[test]
fn downloaded_appimage_is_managed_without_a_temporary_source_reference() {
    let (_directory, service, source) = fixture();
    let app = service.integrate_download(&source).unwrap();

    assert_eq!(
        app.file_path,
        service
            .managed_dir
            .join("Example.AppImage")
            .to_str()
            .unwrap()
    );
    assert!(app.integrated);
    assert!(app.executable);
    assert!(app.source_path.is_none());
    assert!(app.launch_config.is_none());
    assert_eq!(
        fs::read(&app.file_path).unwrap(),
        fs::read(&source).unwrap()
    );
    assert_eq!(service.list().unwrap()[0].source_path, None);
    assert!(app.desktop_id.is_none());
    assert!(!service.desktop_dir.exists());
}

#[test]
fn downloaded_appimage_cannot_overwrite_existing_files_or_import_invalid_content() {
    let (_directory, service, source) = fixture();
    fs::create_dir_all(&service.managed_dir).unwrap();
    let destination = service.managed_dir.join("Example.AppImage");
    fs::write(&destination, b"existing file").unwrap();

    assert!(service
        .integrate_download(&source)
        .unwrap_err()
        .contains("destination already exists"));
    assert_eq!(fs::read(&destination).unwrap(), b"existing file");
    assert!(service.list().unwrap().is_empty());

    fs::write(&source, b"<html>not an AppImage</html>").unwrap();
    assert!(service.integrate_download(&source).is_err());
    assert!(service.list().unwrap().is_empty());
}

#[test]
fn preparing_launch_requires_consent_to_add_execute_permission() {
    let (directory, service, source) = fixture();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);

    assert!(!service.prepare_launch(&app.id, false).unwrap().executable);
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(service.prepare_launch(&app.id, true).unwrap().executable);
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o740
    );
    assert!(service.list().unwrap()[0].executable);

    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!service.prepare_launch(&app.id, false).unwrap().executable);
}

#[test]
fn preparing_launch_rejects_invalid_files_before_changing_permissions() {
    let (directory, service, source) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    fs::write(&source, b"invalid").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();

    assert!(service.prepare_launch(&app.id, true).is_err());
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(service.prepare_launch("missing", true).is_err());
}
