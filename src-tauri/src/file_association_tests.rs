use super::*;
use crate::persistence::tests::TemporaryDirectory;

fn association(directory: &Path) -> Association {
    Association {
        entry: directory.join("applications").join(ENTRY_NAME),
        icon: directory.join("appbento").join("launcher-icon.png"),
    }
}

fn executable(directory: &Path, name: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, b"appimage").unwrap();
    path
}

#[test]
fn registers_appbento_for_appimage_files_without_a_menu_entry() {
    let directory = TemporaryDirectory::new();
    let service = association(&directory.0);
    let appimage = executable(&directory.0, "App Bento\"$`.AppImage");

    assert_eq!(service.ensure(&appimage).unwrap(), Registration::Written);

    let content = std::fs::read_to_string(&service.entry).unwrap();
    assert!(content.contains("MimeType=application/vnd.appimage;application/x-iso9660-appimage;\n"));
    assert!(content.contains(&format!(
        "Exec=\"{}\" --open %f\n",
        desktop::escape_exec(appimage.to_str().unwrap())
    )));
    assert!(content.contains("NoDisplay=true\n"));
    assert!(content.contains(&format!("Icon={}\n", service.icon.display())));
    assert_eq!(std::fs::read(&service.icon).unwrap(), self_launcher::ICON);
}

#[test]
fn startup_check_leaves_a_correct_entry_untouched() {
    let directory = TemporaryDirectory::new();
    let service = association(&directory.0);
    let appimage = executable(&directory.0, "AppBento.AppImage");
    service.ensure(&appimage).unwrap();

    assert_eq!(service.ensure(&appimage).unwrap(), Registration::Unchanged);
}

#[test]
fn startup_check_repairs_a_moved_appimage_and_a_missing_icon() {
    let directory = TemporaryDirectory::new();
    let service = association(&directory.0);
    let old = executable(&directory.0, "AppBento_0.7.0.AppImage");
    let new = executable(&directory.0, "AppBento_0.7.1.AppImage");
    service.ensure(&old).unwrap();
    std::fs::remove_file(&service.icon).unwrap();

    assert_eq!(service.ensure(&new).unwrap(), Registration::Written);

    let content = std::fs::read_to_string(&service.entry).unwrap();
    assert!(content.contains(&format!("Exec=\"{}\" --open %f\n", new.display())));
    assert!(!content.contains(old.to_str().unwrap()));
    assert_eq!(std::fs::read(&service.icon).unwrap(), self_launcher::ICON);
}

#[test]
fn preserves_entries_not_managed_by_appbento() {
    let directory = TemporaryDirectory::new();
    let service = association(&directory.0);
    let appimage = executable(&directory.0, "AppBento.AppImage");
    std::fs::create_dir_all(service.entry.parent().unwrap()).unwrap();
    let foreign = "[Desktop Entry]\nName=Other\nExec=/bin/true\n";
    std::fs::write(&service.entry, foreign).unwrap();

    assert!(service.ensure(&appimage).is_err());
    assert_eq!(std::fs::read_to_string(&service.entry).unwrap(), foreign);
}

#[test]
fn rejects_launchers_that_cannot_be_written_safely() {
    let directory = TemporaryDirectory::new();
    let service = association(&directory.0);
    let with_equals = executable(&directory.0, "app=bento.AppImage");
    for appimage in [
        Path::new("relative.AppImage"),
        &directory.0.join("missing.AppImage"),
        &with_equals,
    ] {
        assert!(service.ensure(appimage).is_err());
    }
    assert!(!service.entry.exists());
}

#[test]
fn reads_the_file_from_an_open_with_launch_only() {
    let arguments = |values: &[&str]| {
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    };

    assert_eq!(
        requested_path(&arguments(&["--open", "/home/me/Example.AppImage"])),
        Some("/home/me/Example.AppImage".into())
    );
    assert_eq!(requested_path(&arguments(&["--open"])), None);
    assert_eq!(requested_path(&arguments(&["--open", ""])), None);
    assert_eq!(requested_path(&arguments(&["--launch", "id"])), None);
    assert_eq!(requested_path(&arguments(&[])), None);
}

#[test]
fn hands_the_requested_file_to_the_window_once() {
    let request = OpenRequest::new(Some("/home/me/Example.AppImage".into()));

    assert_eq!(
        request.take().unwrap(),
        Some("/home/me/Example.AppImage".into())
    );
    assert_eq!(request.take().unwrap(), None);
}
