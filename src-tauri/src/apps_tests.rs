use super::*;
use crate::persistence::tests::TemporaryDirectory;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

const SCAN_TEST_DIRECTORY: &str = "APPBENTO_TEST_ICON_SCAN_DIRECTORY";
const EMBEDDED_ICON: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'><rect width='1' height='1'/></svg>";
const CUSTOM_ICON: &[u8] =
    b"<svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'><circle r='1'/></svg>";

fn create_appimage(directory: &Path) -> PathBuf {
    let root = directory.join("image-root");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("example.desktop"),
        "[Desktop Entry]\nIcon=example\n",
    )
    .unwrap();
    fs::write(root.join("example.svg"), EMBEDDED_ICON).unwrap();
    let filesystem = directory.join("filesystem.squashfs");
    let status = Command::new("/usr/bin/mksquashfs")
        .arg(&root)
        .arg(&filesystem)
        .args(["-noappend", "-processors", "1", "-quiet"])
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let mut image = vec![0; 4096];
    image[..4].copy_from_slice(b"\x7fELF");
    image[4] = 2;
    image[5] = 1;
    image[8..11].copy_from_slice(b"AI\x02");
    let architecture: u16 = if std::env::consts::ARCH == "aarch64" {
        183
    } else {
        62
    };
    image[18..20].copy_from_slice(&architecture.to_le_bytes());
    image.extend(fs::read(filesystem).unwrap());
    let apps = directory.join("apps");
    fs::create_dir(&apps).unwrap();
    let path = apps.join("Existing.AppImage");
    fs::write(&path, image).unwrap();
    path
}

#[test]
fn rescan_backfills_existing_missing_icons_and_preserves_custom_icons() {
    if !Path::new("/usr/bin/mksquashfs").exists() || !Path::new("/usr/bin/unsquashfs").exists() {
        eprintln!("skipping icon scan fixture: squashfs-tools unavailable");
        return;
    }
    let directory = TemporaryDirectory::new();
    let source = create_appimage(&directory.0);
    let apps = directory.0.join("apps");
    let custom_source = apps.join("Custom.AppImage");
    fs::copy(&source, &custom_source).unwrap();
    let data = directory.0.join("data");
    let app_data = data.join("appbento");
    let library_path = app_data.join("library.json");
    let service = Lifecycle {
        library_path: library_path.clone(),
        managed_dir: apps.clone(),
        desktop_dir: data.join("applications"),
    };
    let original = service.scan(vec![apps.clone()]).unwrap();
    assert_eq!(original.apps.len(), 2);
    assert!(original.apps.iter().all(|app| app.icon_path.is_none()));
    icons::persist_icon(
        &library_path,
        &app_data.join("icons"),
        custom_source.to_str().unwrap(),
        CUSTOM_ICON,
        true,
    )
    .unwrap();
    let settings = crate::settings::Settings {
        managed_dir: apps.to_str().unwrap().into(),
        scan_dirs: Vec::new(),
    };
    persistence::mutate(
        &app_data.join("settings.json"),
        |stored: &mut crate::settings::Settings| {
            *stored = settings;
            Ok(())
        },
    )
    .unwrap();
    let home = directory.0.join("home");
    fs::create_dir(&home).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "apps::tests::rescan_icons_worker", "--nocapture"])
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .env(SCAN_TEST_DIRECTORY, &directory.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated scan failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stored: crate::store::Library = persistence::load(&library_path).unwrap();
    assert_eq!(
        fs::read(stored[source.to_str().unwrap()].icon_path.as_ref().unwrap()).unwrap(),
        EMBEDDED_ICON
    );
    assert_eq!(
        fs::read(
            stored[custom_source.to_str().unwrap()]
                .icon_path
                .as_ref()
                .unwrap()
        )
        .unwrap(),
        CUSTOM_ICON
    );
}

#[tokio::test]
async fn rescan_icons_worker() {
    let Some(directory) = std::env::var_os(SCAN_TEST_DIRECTORY) else {
        return;
    };
    let directory = PathBuf::from(directory);
    let apps = directory.join("apps");
    let report = library_scan(Some(vec![apps.to_str().unwrap().into()]))
        .await
        .unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let source = apps.join("Existing.AppImage");
    let found = report
        .apps
        .iter()
        .find(|app| app.file_path == source.to_str().unwrap())
        .unwrap();
    assert!(
        found.icon_path.is_some(),
        "an existing record without an icon should gain its embedded icon on rescan"
    );
    let url = library_icon(found.id.clone()).await.unwrap().unwrap();
    assert!(url.starts_with("data:image/svg+xml;base64,"));
    let custom_source = apps.join("Custom.AppImage");
    let custom = report
        .apps
        .iter()
        .find(|app| app.file_path == custom_source.to_str().unwrap())
        .unwrap();
    assert_eq!(
        fs::read(custom.icon_path.as_ref().unwrap()).unwrap(),
        CUSTOM_ICON
    );
}
