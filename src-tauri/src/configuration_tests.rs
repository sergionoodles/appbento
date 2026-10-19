use super::*;
use crate::lifecycle::Lifecycle;
use crate::models::{
    LaunchConfig, LaunchMode, PathAccess, PathPermission, SandboxPermissions, UpdateConfig,
};
use crate::persistence::tests::TemporaryDirectory;

fn create_appimage(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut header = [0; 20];
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
    fs::write(path, header).unwrap();
}

fn fixture() -> (TemporaryDirectory, ConfigurationBackup) {
    let directory = TemporaryDirectory::new();
    let home = directory.0.join("old-home");
    let source = home.join("Downloads/Example.AppImage");
    create_appimage(&source);
    let data = directory.0.join("old-data");
    let lifecycle = Lifecycle {
        library_path: data.join("library.json"),
        managed_dir: home.join("Applications"),
        desktop_dir: directory.0.join("old-desktop"),
    };
    let app = lifecycle.integrate(&source, false).unwrap();
    let app = lifecycle.set_menu_entry(&app.id, true).unwrap();
    let grant = home.join("Documents");
    fs::create_dir(&grant).unwrap();
    lifecycle
        .configure(
            &app.id,
            &LaunchConfig {
                arguments: vec![
                    "--profile".into(),
                    home.join("profile").to_string_lossy().into_owned(),
                ],
                environment: [
                    (
                        "PROFILE".into(),
                        home.join("profile").to_string_lossy().into_owned(),
                    ),
                    ("LABEL".into(), "literal value".into()),
                ]
                .into(),
                mode: LaunchMode::Bubblewrap,
                permissions: SandboxPermissions {
                    network: true,
                    audio: true,
                    gpu: true,
                    paths: vec![PathPermission {
                        path: grant.to_string_lossy().into_owned(),
                        access: PathAccess::ReadWrite,
                    }],
                    ..Default::default()
                },
            },
        )
        .unwrap();
    crate::icons::persist_icon(
        &lifecycle.library_path,
        &data.join("icons"),
        &app.id,
        b"<svg><rect width='1' height='1'/></svg>",
        true,
    )
    .unwrap();
    persistence::mutate(&lifecycle.library_path, |library: &mut Library| {
        let app = library.get_mut(&app.id).unwrap();
        app.update_config = Some(UpdateConfig::DirectUrl {
            url: "https://example.org/app.AppImage".into(),
        });
        app.version_current = Some("1.2.3".into());
        app.launch_count = 42;
        app.last_launched = Some(chrono::Utc::now());
        Ok(())
    })
    .unwrap();
    persistence::mutate(&data.join("settings.json"), |settings: &mut Settings| {
        settings.managed_dir = lifecycle.managed_dir.to_string_lossy().into_owned();
        settings.scan_dirs = vec![home.join("Downloads").to_string_lossy().into_owned()];
        Ok(())
    })
    .unwrap();
    let preferences = Preferences {
        theme: "paper".into(),
        display_names: [(app.id.clone(), "My example".into())].into(),
        layout: Layout {
            ungrouped: vec![],
            groups: vec![Group {
                id: "group".into(),
                name: "Work".into(),
                app_ids: vec![app.id],
                is_collapsed: true,
            }],
        },
    };
    let backup = export_at(&lifecycle.library_path, preferences, &home).unwrap();
    (directory, backup)
}

fn request(directory: &TemporaryDirectory, backup: ConfigurationBackup) -> RestoreRequest {
    let home = directory.0.join("new-home");
    let preview = preview(backup, &home).unwrap();
    fs::create_dir_all(home.join("Documents")).unwrap();
    for entry in &preview.backup.apps {
        let destination = Path::new(&preview.app_paths[&entry.app.id]);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(&entry.app.file_path, destination).unwrap();
    }
    RestoreRequest {
        backup: preview.backup,
        settings: preview.settings,
        app_paths: preview.app_paths,
        permission_paths: preview.permission_paths,
    }
}

#[test]
fn settings_round_trip_to_another_home_preserves_every_configuration() {
    let (directory, backup) = fixture();
    let original = backup.apps[0].app.clone();
    let backup_file = directory.0.join("backup.json");
    fs::write(&backup_file, serde_json::to_vec(&backup).unwrap()).unwrap();
    let request = request(&directory, read_backup(&backup_file).unwrap());
    let path = directory.0.join("new-data/library.json");
    let preferences = restore_at(
        &path,
        &directory.0.join("new-desktop"),
        &directory.0.join("new-home"),
        request,
    )
    .unwrap();
    let library: Library = persistence::load(&path).unwrap();
    let restored = library.values().next().unwrap();
    assert_eq!(restored.id, restored.file_path);
    assert_ne!(restored.id, original.id);
    assert_eq!(restored.name, original.name);
    assert_eq!(restored.update_config, original.update_config);
    assert_eq!(restored.version_current, original.version_current);
    assert_eq!(restored.launch_count, original.launch_count);
    assert_eq!(restored.last_launched, original.last_launched);
    assert_eq!(restored.added_at, original.added_at);
    assert_eq!(restored.file_created_at, original.file_created_at);
    assert_eq!(restored.reviewed_digest, original.reviewed_digest);
    assert_eq!(
        fs::read(restored.icon_path.as_ref().unwrap()).unwrap(),
        backup.apps[0].icon.as_ref().unwrap().clone()
    );
    let mut expected = original.launch_config.unwrap();
    expected.arguments[1] = directory
        .0
        .join("new-home/profile")
        .to_string_lossy()
        .into_owned();
    expected
        .environment
        .insert("PROFILE".into(), expected.arguments[1].clone());
    expected.permissions.paths[0].path = directory
        .0
        .join("new-home/Documents")
        .to_string_lossy()
        .into_owned();
    assert_eq!(restored.launch_config, Some(expected));
    assert_eq!(preferences.theme, "paper");
    assert_eq!(preferences.display_names[&restored.id], "My example");
    assert_eq!(
        preferences.layout.groups[0].app_ids,
        vec![restored.id.clone()]
    );
    assert!(preferences.layout.groups[0].is_collapsed);
    assert!(restored.desktop_id.is_none());
    assert!(!directory.0.join("new-desktop").exists());
    let settings: Settings =
        persistence::load(&path.parent().unwrap().join("settings.json")).unwrap();
    assert_eq!(
        settings.managed_dir,
        directory.0.join("new-home/Applications").to_string_lossy()
    );
    assert_eq!(
        settings.scan_dirs,
        vec![directory.0.join("new-home/Downloads").to_string_lossy()]
    );
    let lifecycle = Lifecycle {
        library_path: path.clone(),
        managed_dir: settings.managed_dir.into(),
        desktop_dir: directory.0.join("new-desktop"),
    };
    lifecycle
        .scan(vec![directory.0.join("new-home/Applications")])
        .unwrap();
    assert_eq!(
        lifecycle.list().unwrap()[0].launch_config,
        restored.launch_config
    );
    let pending: Option<Preferences> =
        persistence::load(&path.parent().unwrap().join("restored-preferences.json")).unwrap();
    assert_eq!(pending.unwrap().display_names[&restored.id], "My example");
}

#[test]
fn invalid_or_missing_files_do_not_change_existing_settings_or_library() {
    let (directory, backup) = fixture();
    let mut request = request(&directory, backup);
    let path = directory.0.join("new-data/library.json");
    persistence::mutate(&path, |_: &mut Library| Ok(())).unwrap();
    let settings_path = path.parent().unwrap().join("settings.json");
    persistence::mutate(&settings_path, |settings: &mut Settings| {
        settings.managed_dir = "/tmp/existing".into();
        Ok(())
    })
    .unwrap();
    let library_before = fs::read(&path).unwrap();
    let settings_before = fs::read(&settings_path).unwrap();
    request
        .app_paths
        .values_mut()
        .for_each(|value| *value = "/missing/app.AppImage".into());
    assert!(restore_at(
        &path,
        &directory.0.join("new-desktop"),
        &directory.0.join("new-home"),
        request
    )
    .is_err());
    assert_eq!(fs::read(path).unwrap(), library_before);
    assert_eq!(fs::read(settings_path).unwrap(), settings_before);
    assert!(!directory.0.join("new-desktop").exists());
}

#[test]
fn restoring_an_app_without_a_shortcut_does_not_create_one() {
    let (directory, mut backup) = fixture();
    backup.apps[0].app.desktop_id = None;
    let request = request(&directory, backup);
    let path = directory.0.join("new-data/library.json");
    let desktop = directory.0.join("new-desktop");
    restore_at(&path, &desktop, &directory.0.join("new-home"), request).unwrap();
    let library: Library = persistence::load(&path).unwrap();
    assert!(library.values().next().unwrap().desktop_id.is_none());
    assert!(!desktop.exists());
}

#[test]
fn unsupported_formats_duplicate_ids_and_invalid_icons_are_rejected() {
    let (_directory, mut backup) = fixture();
    backup.version += 1;
    assert!(validate_backup(&backup).is_err());
    backup.version = FORMAT_VERSION;
    backup.apps.push(backup.apps[0].clone());
    assert!(validate_backup(&backup).is_err());
    backup.apps.pop();
    backup.apps[0].icon = Some(b"invalid image".to_vec());
    assert!(validate_backup(&backup).is_err());
}

#[test]
fn changed_appimage_keeps_settings_and_requires_a_new_review_even_after_scan() {
    let (directory, backup) = fixture();
    let request = request(&directory, backup);
    let image = request.app_paths.values().next().unwrap();
    let mut bytes = fs::read(image).unwrap();
    bytes.extend(b"changed binary");
    fs::write(image, bytes).unwrap();
    let path = directory.0.join("new-data/library.json");
    restore_at(
        &path,
        &directory.0.join("new-desktop"),
        &directory.0.join("new-home"),
        request,
    )
    .unwrap();
    let lifecycle = Lifecycle {
        library_path: path,
        managed_dir: directory.0.join("new-home/Applications"),
        desktop_dir: directory.0.join("new-desktop"),
    };
    let before = lifecycle.list().unwrap().remove(0);
    assert!(before.reviewed_digest.is_none());
    assert!(before.launch_config.is_some());
    let after = lifecycle
        .scan(vec![lifecycle.managed_dir.clone()])
        .unwrap()
        .apps
        .remove(0);
    assert_eq!(after.launch_config, before.launch_config);
    assert!(after.reviewed_digest.is_none());
}

#[test]
fn path_remapping_respects_home_boundaries_and_leaves_literal_values_unchanged() {
    assert_eq!(
        remap_path("/home/old/Documents", "/home/old", Path::new("/home/new")),
        "/home/new/Documents"
    );
    for value in [
        "/home/older/Documents",
        "literal value",
        "--file=/home/old/file",
    ] {
        assert_eq!(
            remap_path(value, "/home/old", Path::new("/home/new")),
            value
        );
    }
}

#[test]
fn restoring_existing_app_keeps_its_identity_and_unrelated_library_entries() {
    let (directory, backup) = fixture();
    let request = request(&directory, backup.clone());
    let image = request.app_paths.values().next().unwrap().clone();
    let path = directory.0.join("new-data/library.json");
    let mut existing = backup.apps[0].app.clone();
    existing.id = "stable-existing-id".into();
    existing.file_path = image;
    existing.launch_count = 1;
    let mut unrelated = existing.clone();
    unrelated.id = "other-id".into();
    unrelated.file_path = directory
        .0
        .join("other.AppImage")
        .to_string_lossy()
        .into_owned();
    persistence::mutate(&path, |library: &mut Library| {
        library.insert(existing.id.clone(), existing.clone());
        library.insert(unrelated.id.clone(), unrelated.clone());
        Ok(())
    })
    .unwrap();
    let preferences = restore_at(
        &path,
        &directory.0.join("new-desktop"),
        &directory.0.join("new-home"),
        request,
    )
    .unwrap();
    let library: Library = persistence::load(&path).unwrap();
    assert_eq!(library.len(), 2);
    assert_eq!(library["stable-existing-id"].launch_count, 42);
    assert_eq!(library["other-id"], unrelated);
    assert_eq!(
        preferences.layout.groups[0].app_ids,
        vec!["stable-existing-id"]
    );
}

#[test]
fn missing_permission_folders_and_duplicate_file_mapping_are_rejected_before_writes() {
    let (directory, backup) = fixture();
    let mut request = request(&directory, backup.clone());
    request.permission_paths.values_mut().for_each(|value| {
        *value = directory
            .0
            .join("missing-folder")
            .to_string_lossy()
            .into_owned()
    });
    let path = directory.0.join("new-data/library.json");
    assert!(restore_at(
        &path,
        &directory.0.join("new-desktop"),
        &directory.0.join("new-home"),
        request
    )
    .is_err());
    assert!(!path.exists());
    let mut request = self::request(&directory, backup);
    let mut duplicate = request.backup.apps[0].clone();
    duplicate.app.id = "another-id".into();
    request.app_paths.insert(
        duplicate.app.id.clone(),
        request.app_paths.values().next().unwrap().clone(),
    );
    request.backup.apps.push(duplicate);
    assert!(restore_at(
        &path,
        &directory.0.join("new-desktop"),
        &directory.0.join("new-home"),
        request
    )
    .is_err());
    assert!(!path.exists());
}

#[test]
fn interrupted_restore_replays_all_writes_before_next_library_read() {
    let directory = TemporaryDirectory::new();
    let data = directory.0.join("data");
    let blocked = directory.0.join("blocked");
    fs::write(&blocked, "temporarily unavailable directory").unwrap();
    let path = data.join("library.json");
    persistence::with_repository(&path, |_: &mut Repository<Library>| {
        let result = persistence::commit_file_batch(
            &data,
            vec![
                FileChange::Write {
                    path: data.join("settings.json"),
                    bytes: br#"{"managed_dir":"/tmp/restored","scan_dirs":[]}"#.to_vec(),
                },
                FileChange::Write {
                    path: blocked.join("app.desktop"),
                    bytes: b"desktop entry".to_vec(),
                },
                FileChange::Write {
                    path: path.clone(),
                    bytes: b"{}".to_vec(),
                },
            ],
        );
        assert!(result.unwrap_err().contains("pending"));
        Ok(())
    })
    .unwrap();
    assert!(data.join("configuration-restore.json").exists());
    assert!(!path.exists());
    fs::remove_file(&blocked).unwrap();
    let library: Library = persistence::load(&path).unwrap();
    assert!(library.is_empty());
    assert_eq!(
        fs::read_to_string(blocked.join("app.desktop")).unwrap(),
        "desktop entry"
    );
    assert!(!data.join("configuration-restore.json").exists());
    let settings: Settings = persistence::load(&data.join("settings.json")).unwrap();
    assert_eq!(settings.managed_dir, "/tmp/restored");
}

#[test]
fn restore_preserves_foreign_desktop_entries_even_for_known_integrated_apps() {
    let (directory, backup) = fixture();
    let request = request(&directory, backup.clone());
    let image = request.app_paths.values().next().unwrap().clone();
    let path = directory.0.join("new-data/library.json");
    let mut existing = backup.apps[0].app.clone();
    existing.id = image.clone();
    existing.file_path = image.clone();
    existing.desktop_id = Some(crate::lifecycle::desktop::desktop_id(Path::new(&image)));
    persistence::mutate(&path, |library: &mut Library| {
        library.insert(existing.id.clone(), existing.clone());
        Ok(())
    })
    .unwrap();
    let desktop = directory.0.join("new-desktop");
    fs::create_dir(&desktop).unwrap();
    let entry = desktop.join(crate::lifecycle::desktop::desktop_id(Path::new(&image)));
    fs::write(&entry, "[Desktop Entry]\nExec=foreign\n").unwrap();
    let before = fs::read(&path).unwrap();
    assert!(restore_at(&path, &desktop, &directory.0.join("new-home"), request).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(
        fs::read_to_string(entry).unwrap(),
        "[Desktop Entry]\nExec=foreign\n"
    );
}

#[test]
fn restores_multiple_app_order_and_group_order_without_alphabetical_reordering() {
    let (directory, mut backup) = fixture();
    let first_id = backup.apps[0].app.id.clone();
    let mut second = backup.apps[0].clone();
    second.app.id = "second-old-id".into();
    second.app.name = "Another app".into();
    second.app.file_path = directory
        .0
        .join("old-home/Applications/Another.AppImage")
        .to_string_lossy()
        .into_owned();
    create_appimage(Path::new(&second.app.file_path));
    let second_id = second.app.id.clone();
    backup.apps.push(second);
    backup.preferences.layout = Layout {
        ungrouped: vec![second_id.clone(), first_id.clone()],
        groups: vec![
            Group {
                id: "second-group".into(),
                name: "Second group".into(),
                app_ids: vec![],
                is_collapsed: false,
            },
            Group {
                id: "first-group".into(),
                name: "First group".into(),
                app_ids: vec![],
                is_collapsed: true,
            },
        ],
    };
    let request = request(&directory, backup);
    let expected = vec![
        request.app_paths[&second_id].clone(),
        request.app_paths[&first_id].clone(),
    ];
    let preferences = restore_at(
        &directory.0.join("new-data/library.json"),
        &directory.0.join("new-desktop"),
        &directory.0.join("new-home"),
        request,
    )
    .unwrap();
    assert_eq!(preferences.layout.ungrouped, expected);
    assert_eq!(
        preferences
            .layout
            .groups
            .iter()
            .map(|group| group.id.as_str())
            .collect::<Vec<_>>(),
        vec!["second-group", "first-group"]
    );
    assert!(preferences.layout.groups[1].is_collapsed);
}
