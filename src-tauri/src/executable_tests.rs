use super::*;
use crate::models::AppKind;
use crate::persistence::tests::TemporaryDirectory;

pub(crate) fn binary_bytes() -> Vec<u8> {
    let mut bytes = vec![0; 64];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[16..18].copy_from_slice(&2_u16.to_le_bytes());
    let machine: u16 = if std::env::consts::ARCH == "aarch64" {
        183
    } else {
        62
    };
    bytes[18..20].copy_from_slice(&machine.to_le_bytes());
    bytes[20..24].copy_from_slice(&1_u32.to_le_bytes());
    bytes[52..54].copy_from_slice(&64_u16.to_le_bytes());
    bytes[54..56].copy_from_slice(&56_u16.to_le_bytes());
    bytes
}

pub(crate) fn binary_fixture() -> (TemporaryDirectory, Lifecycle, PathBuf) {
    let (directory, mut service, _) = super::tests::fixture();
    service.library_path = directory.0.join("policy/library.json");
    fs::create_dir(directory.0.join("bundle")).unwrap();
    let path = directory.0.join("bundle/Example-1.2.3-x86_64");
    fs::write(&path, binary_bytes()).unwrap();
    (directory, service, path)
}

#[test]
fn validates_extensionless_binary_without_execute_permission_and_linux_abi() {
    let (_directory, _service, path) = binary_fixture();
    assert_eq!(app_file::detect(&path).unwrap(), AppKind::Binary);
    assert!(open_app(&path, AppKind::Binary).is_ok());
    let mut bytes = binary_bytes();
    bytes[7] = 3;
    fs::write(&path, bytes).unwrap();
    assert!(open_app(&path, AppKind::Binary).is_ok());
    let mut bytes = binary_bytes();
    bytes[16..18].copy_from_slice(&3_u16.to_le_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(open_app(&path, AppKind::Binary).is_ok());
}

#[test]
fn rejects_invalid_elf_headers_and_never_falls_back_for_appimage_magic() {
    let (_directory, _service, path) = binary_fixture();
    for (offset, value) in [
        (4, 1),
        (5, 2),
        (6, 2),
        (7, 9),
        (16, 1),
        (18, 0),
        (20, 2),
        (54, 1),
        (56, 255),
    ] {
        let mut bytes = binary_bytes();
        bytes[offset] = value;
        fs::write(&path, bytes).unwrap();
        assert!(app_file::detect(&path).is_err(), "offset {offset}");
    }
    let mut bytes = binary_bytes();
    bytes[8..11].copy_from_slice(b"AI\x09");
    fs::write(&path, bytes).unwrap();
    assert!(app_file::detect(&path).unwrap_err().contains("AppImage"));
    fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
    assert!(app_file::detect(&path).unwrap_err().contains("Scripts"));
    fs::write(&path, b"\x7fELF").unwrap();
    assert!(app_file::detect(&path)
        .unwrap_err()
        .contains("not a supported Linux executable"));
}

pub(crate) fn bytes_with_loader(loader: &[u8]) -> Vec<u8> {
    let mut bytes = binary_bytes();
    bytes[32..40].copy_from_slice(&64_u64.to_le_bytes());
    bytes[56..58].copy_from_slice(&1_u16.to_le_bytes());
    bytes.resize(120, 0);
    bytes[64..68].copy_from_slice(&3_u32.to_le_bytes());
    bytes[72..80].copy_from_slice(&120_u64.to_le_bytes());
    bytes[96..104].copy_from_slice(&(loader.len() as u64).to_le_bytes());
    bytes.extend_from_slice(loader);
    bytes
}

#[test]
fn validates_program_table_bounds_and_loader_paths() {
    let (_directory, _service, path) = binary_fixture();
    let missing = b"/nonexistent/appbento-test-loader\0";
    fs::write(&path, bytes_with_loader(missing)).unwrap();
    let error = app_file::detect(&path).unwrap_err();
    assert!(
        error.contains("requires the program loader /nonexistent/appbento-test-loader"),
        "{error}"
    );
    for loader in [
        b"relative\0".as_slice(),
        b"/unterminated",
        b"/embedded\0nul\0",
    ] {
        fs::write(&path, bytes_with_loader(loader)).unwrap();
        assert!(app_file::detect(&path).is_err());
    }
    let mut oversized = binary_bytes();
    oversized[32..40].copy_from_slice(&64_u64.to_le_bytes());
    oversized[56..58].copy_from_slice(&1025_u16.to_le_bytes());
    oversized.resize(64 + 56 * 1025, 0);
    fs::write(&path, oversized).unwrap();
    assert!(app_file::detect(&path).unwrap_err().contains("oversized"));
    let mut bytes = bytes_with_loader(b"/bin/sh\0");
    fs::write(&path, &bytes).unwrap();
    assert!(app_file::detect(&path).is_ok());
    bytes[96..104].copy_from_slice(&4097_u64.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    assert!(app_file::detect(&path).is_err());
    bytes[32..40].copy_from_slice(&u64::MAX.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    assert!(app_file::detect(&path).is_err());
    let link = path.with_file_name("link");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(app_file::detect(&link).unwrap_err().contains("non-symlink"));
}

#[test]
fn legacy_kinds_roundtrip_and_unknown_kind_preserves_library_on_write_attempt() {
    let (directory, service, _) = super::tests::fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let app = service
        .configure(
            &app.id,
            &crate::models::LaunchConfig {
                mode: crate::models::LaunchMode::Direct,
                arguments: vec!["preserved".into()],
                environment: Default::default(),
                permissions: Default::default(),
            },
            Some(&service.prepare_review(&app.id).unwrap()),
        )
        .unwrap();
    let mut value = serde_json::to_value(&app).unwrap();
    value.as_object_mut().unwrap().remove("kind");
    let legacy: InstalledApp = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(legacy.kind, AppKind::AppImage);
    assert_eq!(legacy, app);
    for kind in [AppKind::AppImage, AppKind::Binary] {
        let app = InstalledApp {
            kind,
            ..legacy.clone()
        };
        let loaded: InstalledApp =
            serde_json::from_slice(&serde_json::to_vec(&app).unwrap()).unwrap();
        assert_eq!(loaded, app);
    }
    value["kind"] = "future-kind".into();
    let bytes = serde_json::to_vec(&std::collections::HashMap::from([("app", value)])).unwrap();
    fs::write(&service.library_path, &bytes).unwrap();
    let error = service.scan(vec![]).unwrap_err();
    assert!(error.contains("invalid app kind: future-kind"), "{error}");
    assert_eq!(fs::read(&service.library_path).unwrap(), bytes);
}

#[test]
fn imports_in_place_preserves_settings_and_survives_scans_and_reload() {
    let (directory, service, path) = binary_fixture();
    let app = service.import_app(&path, false).unwrap();
    assert_eq!(app.kind, AppKind::Binary);
    assert_eq!(app.file_path, path.to_str().unwrap());
    assert_eq!(app.name, "Example");
    assert_eq!(app.version_current.as_deref(), Some("1.2.3"));
    assert!(!app.integrated && !app.executable);
    assert!(app.source_path.is_none() && app.update_config.is_none());
    assert!(app.launch_config.is_none() && app.reviewed_digest.is_none());
    let config = crate::models::LaunchConfig {
        mode: crate::models::LaunchMode::Direct,
        arguments: vec!["a literal argument".into()],
        environment: Default::default(),
        permissions: crate::sandbox::default_permissions(&app, &directory.0.join("policy")),
    };
    let configured = service
        .configure(
            &app.id,
            &config,
            Some(&service.prepare_review(&app.id).unwrap()),
        )
        .unwrap();
    let prepared = service.prepare_launch(&app.id, true).unwrap();
    assert!(prepared.executable);
    let duplicate = service.import_app(&path, false).unwrap();
    assert_eq!(duplicate.launch_config, configured.launch_config);
    assert_eq!(duplicate.reviewed_digest, configured.reviewed_digest);
    service.scan(vec![directory.0.clone()]).unwrap();
    let apps = Lifecycle::list_at(&service.library_path).unwrap();
    assert_eq!(
        apps.iter()
            .filter(|entry| entry.kind == AppKind::Binary)
            .count(),
        1
    );
    assert_eq!(
        apps.iter()
            .find(|entry| entry.id == app.id)
            .unwrap()
            .reviewed_digest,
        configured.reviewed_digest
    );
    let mut bytes = binary_bytes();
    bytes.push(1);
    fs::write(&path, bytes).unwrap();
    assert!(service
        .import_app(&path, false)
        .unwrap()
        .reviewed_digest
        .is_none());
    fs::remove_file(&path).unwrap();
    let error = service.prepare_launch(&app.id, false).unwrap_err();
    assert!(
        error.contains("add it again from its new location"),
        "{error}"
    );
    assert!(!error.contains("scan"));
}

#[test]
fn binaries_cannot_move_be_owned_or_be_deleted_even_with_a_corrupt_ownership_flag() {
    let (_directory, service, path) = binary_fixture();
    assert!(service
        .import_app(&path, true)
        .unwrap_err()
        .contains("cannot be moved"));
    assert!(path.exists());
    assert!(!service.managed_dir.exists());
    fs::create_dir(&service.managed_dir).unwrap();
    let managed = service.managed_dir.join("binary");
    fs::copy(&path, &managed).unwrap();
    let app = service.import_app(&managed, false).unwrap();
    assert!(!app.integrated);
    assert!(service.remove(&app.id, true).is_err());
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.get_mut(&app.id).unwrap().integrated = true;
        Ok(())
    })
    .unwrap();
    assert!(service.remove(&app.id, true).is_err());
    assert!(managed.exists());
    service.remove(&app.id, false).unwrap();
    assert!(managed.exists());
}

#[test]
fn rejects_format_changes_and_scan_kind_mismatches_without_mutating_entries() {
    let (_directory, service, path) = binary_fixture();
    let path = path.with_extension("AppImage");
    fs::write(&path, binary_bytes()).unwrap();
    let app = service.import_app(&path, false).unwrap();
    let mut bytes = binary_bytes();
    bytes[8..11].copy_from_slice(b"AI\x02");
    fs::write(&path, bytes).unwrap();
    assert!(service
        .import_app(&path, false)
        .unwrap_err()
        .contains("remove the old entry"));
    let report = service
        .scan(vec![path.parent().unwrap().to_owned()])
        .unwrap();
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.message.contains("kind differs")));
    assert_eq!(
        service
            .list()
            .unwrap()
            .iter()
            .find(|entry| entry.id == app.id)
            .unwrap(),
        &app
    );
    service.remove(&app.id, false).unwrap();
    let image = service.import_app(&path, false).unwrap();
    fs::write(&path, binary_bytes()).unwrap();
    assert!(service.import_app(&path, false).is_err());
    assert_eq!(
        service
            .list()
            .unwrap()
            .iter()
            .find(|entry| entry.id == image.id)
            .unwrap()
            .kind,
        AppKind::AppImage
    );
}

#[test]
fn binary_menu_entries_use_the_reviewed_launcher_and_leave_the_file_alone() {
    let (_directory, service, path) = binary_fixture();
    let app = service.import_app(&path, false).unwrap();
    let app = service.set_menu_entry(&app.id, true).unwrap();
    assert!(app.launch_config.is_none() && app.reviewed_digest.is_none());
    let entry = fs::read_to_string(service.desktop_dir.join(app.desktop_id.unwrap())).unwrap();
    assert!(entry.contains("--launch"));
    assert!(entry.contains(&app.id));
    service.remove(&app.id, false).unwrap();
    assert!(path.exists());
}

#[tokio::test]
async fn executable_updates_are_rejected_before_network_storage_or_replacement() {
    let (_directory, service, path) = binary_fixture();
    let mut app = service.import_app(&path, false).unwrap();
    app.update_config = Some(crate::models::UpdateConfig::DirectUrl {
        url: "https://127.0.0.1:1/must-not-request".into(),
    });
    let check = crate::updates::check_app(&app).await;
    assert_eq!(check.status, crate::models::UpdateStatus::Unsupported);
    assert_eq!(
        check.error.as_deref(),
        Some("Executable updates are not supported yet")
    );
    let report = crate::updates::check_all(vec![app.clone()]).await;
    assert_eq!(report.unsupported_count, 1);
    assert_eq!(report.checked_count, 0);
    assert!(service
        .replace_update(&app, Path::new("/missing"), "test.AppImage", None, "digest")
        .unwrap_err()
        .contains("Executable updates"));
    assert!(crate::update_download::install(app, service)
        .await
        .unwrap_err()
        .contains("Executable updates"));
    assert!(path.exists());
}
