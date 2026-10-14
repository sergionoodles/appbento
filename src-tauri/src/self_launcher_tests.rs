use super::*;
use crate::persistence::tests::TemporaryDirectory;

fn launcher(directory: &Path) -> Launcher {
    Launcher {
        preferences: directory.join("self-launcher.json"),
        entry: directory.join("applications").join(ENTRY_NAME),
        icon: directory.join("appbento").join("launcher-icon.png"),
    }
}

#[test]
fn dismissal_survives_restart_without_installing() {
    let directory = TemporaryDirectory::new();
    let service = launcher(&directory.0);
    let initial = service.status().unwrap();
    assert!(!initial.installed);
    assert!(!initial.dismissed);
    service.dismiss().unwrap();
    let restarted = launcher(&directory.0).status().unwrap();
    assert!(restarted.dismissed);
    assert!(!restarted.installed);
    assert!(!service.entry.exists());
}

#[test]
fn installs_a_direct_launcher_with_icon_and_detects_it_after_restart() {
    let directory = TemporaryDirectory::new();
    let service = launcher(&directory.0);
    let executable = directory.0.join("app bento\"$`\\.AppImage");
    std::fs::write(&executable, b"appimage").unwrap();
    let status = service.install(&executable).unwrap();
    assert!(status.installed);
    assert!(status.dismissed);
    let content = std::fs::read_to_string(&service.entry).unwrap();
    assert!(content.contains("Name=appbento\n"));
    assert!(content.contains(&format!(
        "Exec=\"{}\"\n",
        desktop::escape_exec(executable.to_str().unwrap())
    )));
    assert!(!content.contains("--launch"));
    assert!(content.contains(&format!("Icon={}\n", service.icon.display())));
    assert_eq!(std::fs::read(&service.icon).unwrap(), ICON);
    assert!(launcher(&directory.0).status().unwrap().installed);
    std::fs::remove_file(&service.entry).unwrap();
    let removed = service.status().unwrap();
    assert!(!removed.installed);
    assert!(removed.dismissed);
}

#[test]
fn preserves_foreign_entries_and_does_not_mark_installation_complete() {
    let directory = TemporaryDirectory::new();
    let service = launcher(&directory.0);
    let executable = directory.0.join("appbento.AppImage");
    std::fs::write(&executable, b"appimage").unwrap();
    std::fs::create_dir_all(service.entry.parent().unwrap()).unwrap();
    let foreign = "[Desktop Entry]\nName=Existing app\nExec=/bin/true\n";
    std::fs::write(&service.entry, foreign).unwrap();
    assert!(service.install(&executable).is_err());
    assert_eq!(std::fs::read_to_string(&service.entry).unwrap(), foreign);
    assert!(!service.status().unwrap().dismissed);
    assert!(!service.status().unwrap().installed);
}

#[test]
fn failed_write_keeps_the_launcher_available() {
    let directory = TemporaryDirectory::new();
    let service = launcher(&directory.0);
    let executable = directory.0.join("appbento.AppImage");
    std::fs::write(&executable, b"appimage").unwrap();
    std::fs::write(service.entry.parent().unwrap(), b"blocked").unwrap();
    assert!(service.install(&executable).is_err());
    let preferences: Preferences = persistence::load(&service.preferences).unwrap();
    assert!(!preferences.dismissed);
}

#[test]
fn rejects_invalid_executables_before_writing_any_files() {
    let directory = TemporaryDirectory::new();
    let service = launcher(&directory.0);
    for executable in [Path::new("relative"), &directory.0.join("missing.AppImage")] {
        assert!(service.install(executable).is_err());
    }
    assert!(!service.entry.exists());
    assert!(!service.icon.exists());
}
