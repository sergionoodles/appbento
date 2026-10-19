use super::*;
use crate::icons;
use std::process::{Command, Stdio};

const OLD_ICON: &[u8] = b"<svg width=\"1\" height=\"1\"/>";
const NEW_ICON: &[u8] = b"<svg width=\"2\" height=\"2\"/>";
const RUNTIME_LENGTH: usize = 4096;

fn metadata_update(directory: &Path, source: &Path, icon: &[u8]) -> Option<PathBuf> {
    if !Path::new("/usr/bin/mksquashfs").exists() || !Path::new("/usr/bin/unsquashfs").exists() {
        eprintln!("skipping fixture: squashfs-tools unavailable");
        return None;
    }

    let root = directory.join("metadata-root");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("example.desktop"),
        "[Desktop Entry]\nName=Updated Example Portable\nIcon=example\nX-AppImage-Version=2.0.0\n",
    )
    .unwrap();
    fs::write(root.join("example.svg"), icon).unwrap();
    let filesystem = directory.join("metadata.squashfs");
    let status = Command::new("/usr/bin/mksquashfs")
        .arg(root)
        .arg(&filesystem)
        .args(["-noappend", "-processors", "1", "-quiet"])
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let mut bytes = fs::read(source).unwrap();
    bytes.resize(RUNTIME_LENGTH, 0);
    bytes.extend(fs::read(filesystem).unwrap());
    let staged = directory.join("download.tmp");
    fs::write(&staged, bytes).unwrap();
    Some(staged)
}

fn save_old_icon(service: &Lifecycle, app: &InstalledApp) -> InstalledApp {
    icons::persist_icon(
        &service.library_path,
        &service.library_path.parent().unwrap().join("icons"),
        &app.id,
        OLD_ICON,
        true,
    )
    .unwrap()
}

#[test]
fn update_refreshes_embedded_metadata_and_saved_icon() {
    for is_integrated in [false, true] {
        let (directory, service, source) = tests::fixture();
        let app = if is_integrated {
            service.integrate(&source).unwrap()
        } else {
            service
                .scan(vec![directory.0.clone()])
                .unwrap()
                .apps
                .remove(0)
        };
        let app = save_old_icon(&service, &app);
        let digest = crate::sandbox::appimage_digest(Path::new(&app.file_path)).unwrap();
        let Some(staged) = metadata_update(&directory.0, &source, NEW_ICON) else {
            return;
        };

        let updated = service
            .replace_update(&app, &staged, "Example.AppImage", Some("3.0.0"), &digest)
            .unwrap();

        assert_ne!(updated.icon_path, app.icon_path);
        assert_eq!(
            fs::read(updated.icon_path.as_ref().unwrap()).unwrap(),
            NEW_ICON
        );
        assert_eq!(fs::read(app.icon_path.as_ref().unwrap()).unwrap(), OLD_ICON);
        assert_eq!(updated.name, "Updated Example");
        assert_eq!(updated.version_current.as_deref(), Some("3.0.0"));
        assert_eq!(updated.id, app.id);
        assert_eq!(updated.added_at, app.added_at);
        assert_eq!(updated.launch_count, app.launch_count);
        assert_eq!(updated.desktop_id, app.desktop_id);
        assert_eq!(service.list().unwrap(), vec![updated]);
    }
}

#[test]
fn direct_update_reads_embedded_version_and_keeps_icon_when_new_icon_is_invalid() {
    let (directory, service, source) = tests::fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let app = save_old_icon(&service, &app);
    let digest = crate::sandbox::appimage_digest(&source).unwrap();
    let Some(staged) = metadata_update(&directory.0, &source, b"invalid icon") else {
        return;
    };

    let updated = service
        .replace_update(&app, &staged, "Example.AppImage", None, &digest)
        .unwrap();

    assert_eq!(updated.version_current.as_deref(), Some("2.0.0"));
    assert_eq!(updated.name, "Updated Example");
    assert_eq!(updated.icon_path, app.icon_path);
    assert_eq!(service.list().unwrap(), vec![updated]);
}

#[test]
fn update_keeps_saved_metadata_when_embedded_metadata_is_unavailable() {
    let (directory, service, source) = tests::fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let app = save_old_icon(&service, &app);
    let digest = crate::sandbox::appimage_digest(&source).unwrap();
    let staged = tests::staged_update(&directory.0, &source);

    let updated = service
        .replace_update(&app, &staged, "Example.AppImage", None, &digest)
        .unwrap();

    assert_eq!(updated.name, app.name);
    assert_eq!(updated.version_current, app.version_current);
    assert_eq!(updated.icon_path, app.icon_path);
    assert_eq!(service.list().unwrap(), vec![updated]);
}

#[test]
fn failed_icon_save_leaves_installed_app_and_library_unchanged() {
    let (directory, service, source) = tests::fixture();
    let app = service.integrate(&source).unwrap();
    let before = fs::read(&app.file_path).unwrap();
    let digest = crate::sandbox::appimage_digest(Path::new(&app.file_path)).unwrap();
    let Some(staged) = metadata_update(&directory.0, &source, NEW_ICON) else {
        return;
    };
    fs::write(directory.0.join("icons"), b"blocked icon directory").unwrap();

    assert!(service
        .replace_update(&app, &staged, "Example.AppImage", Some("2.0.0"), &digest)
        .is_err());

    assert_eq!(fs::read(&app.file_path).unwrap(), before);
    assert_eq!(service.list().unwrap(), vec![app]);
}
