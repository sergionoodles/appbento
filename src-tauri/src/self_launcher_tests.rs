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
    assert!(content.contains("Name=AppBento\n"));
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

#[test]
fn updates_only_launcher_paths_and_can_restore_the_previous_entry() {
    let directory = TemporaryDirectory::new();
    let service = launcher(&directory.0);
    let old = directory.0.join("AppBento_0.5.0.AppImage");
    let new = directory.0.join("AppBento 0.6.0.AppImage");
    std::fs::write(&old, b"old image").unwrap();
    std::fs::write(&new, b"new image").unwrap();
    service.install(&old).unwrap();
    let previous = std::fs::read_to_string(&service.entry).unwrap() + "StartupWMClass=appbento\n";
    std::fs::write(&service.entry, &previous).unwrap();
    let update = service.update_executable(&new).unwrap().unwrap();
    let updated = std::fs::read_to_string(&service.entry).unwrap();
    assert!(updated.contains(&format!("Exec=\"{}\"\n", new.display())));
    assert!(!updated.contains(old.to_str().unwrap()));
    assert!(updated.contains("StartupWMClass=appbento\n"));
    update.rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&service.entry).unwrap(), previous);
}

#[test]
fn updating_does_not_install_a_dismissed_launcher_or_overwrite_foreign_entries() {
    let directory = TemporaryDirectory::new();
    let service = launcher(&directory.0);
    let executable = directory.0.join("new.AppImage");
    std::fs::write(&executable, b"new image").unwrap();
    service.dismiss().unwrap();
    assert!(service.update_executable(&executable).unwrap().is_none());
    assert!(!service.entry.exists());
    std::fs::create_dir_all(service.entry.parent().unwrap()).unwrap();
    let foreign = "[Desktop Entry]\nName=Other\nExec=/bin/true\n";
    std::fs::write(&service.entry, foreign).unwrap();
    assert!(service.update_executable(&executable).is_err());
    assert_eq!(std::fs::read_to_string(&service.entry).unwrap(), foreign);
}

#[test]
fn failed_launch_restores_the_launcher_and_preserves_the_running_image() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TemporaryDirectory::new();
    let service = launcher(&directory.0);
    let current = directory.0.join("old.AppImage");
    let staging = directory.0.join("staging.AppImage");
    std::fs::write(&current, b"old image").unwrap();
    service.install(&current).unwrap();
    let previous = std::fs::read_to_string(&service.entry).unwrap();
    // This exercises the real spawn failure before any child can migrate library entries.
    let result = crate::self_update_installation::install_and_launch(
        &current,
        &staging,
        "0.6.0",
        |path| {
            std::fs::write(path, b"not an executable").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                .map_err(|error| error.to_string())
        },
        |path| service.update_executable(path),
        || Ok(()),
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&service.entry).unwrap(), previous);
    assert_eq!(std::fs::read(&current).unwrap(), b"old image");
    assert!(!staging.exists());
}
