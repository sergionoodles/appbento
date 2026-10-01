use super::*;
use crate::persistence::tests::TemporaryDirectory;

const ELF_HEADER_LENGTH: usize = 20;

fn fixture() -> (TemporaryDirectory, Lifecycle, PathBuf) {
    let directory = TemporaryDirectory::new();
    let service = Lifecycle {
        library_path: directory.0.join("library.json"),
        managed_dir: directory.0.join("managed"),
        desktop_dir: directory.0.join("desktop"),
        catalog: Vec::new(),
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
    let app = service.integrate(&source).unwrap();
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
    assert!(service.integrate(&source).is_err());
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
    assert!(service.integrate(&source).is_err());
    assert!(!destination.exists());
    assert_eq!(fs::read_to_string(desktop).unwrap(), "foreign launcher");
    assert!(source.exists());
}

#[test]
fn remove_owned_app_cleans_desktop_and_preserves_original() {
    let (_directory, service, source) = fixture();
    let app = service.integrate(&source).unwrap();
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
    let content = desktop_content(path, "name\nExec=bad");
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
    let app = service.integrate(&destination).unwrap();
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
fn catalog_desktop_hints_are_applied() {
    let (_directory, mut service, source) = fixture();
    let renamed = source.with_file_name("Neovim-0.10.0.AppImage");
    fs::rename(&source, &renamed).unwrap();
    service.catalog = catalog::load_catalog().unwrap();
    let app = service.integrate(&renamed).unwrap();
    let desktop = fs::read_to_string(service.desktop_dir.join(app.desktop_id.unwrap())).unwrap();
    assert!(desktop.contains("Terminal=true\n"));
    assert!(desktop.contains("Categories=Development;\n"));
    service.remove(&app.id, false).unwrap();
}

#[test]
fn removal_after_managed_folder_change_uses_stored_ownership() {
    let (directory, mut service, source) = fixture();
    let app = service.integrate(&source).unwrap();
    service.managed_dir = directory.0.join("new-missing-folder");
    service.remove(&app.id, true).unwrap();
    assert!(!Path::new(&app.file_path).exists());
}

#[test]
fn integration_cleanup_does_not_delete_appimage_with_temporary_extension() {
    let (_directory, service, source) = fixture();
    let renamed = source.with_extension("tmp");
    fs::rename(source, &renamed).unwrap();
    let app = service.integrate(&renamed).unwrap();
    assert!(Path::new(&app.file_path).exists());
}

#[path = "lifecycle_recovery_tests.rs"]
mod recovery_tests;
