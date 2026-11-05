//! Detected update sources follow the origin rules when apps are imported and rescanned.
use super::tests::fixture;
use super::*;
use crate::elf_sections::synthetic_elf;
use crate::models::{UpdateConfig, UpdateConfigOrigin};

fn zsync(name: &str) -> UpdateConfig {
    UpdateConfig::Zsync {
        url: format!("https://example.org/{name}.zsync"),
    }
}

fn write_declaring(path: &Path, name: &str) {
    let info = format!("zsync|https://example.org/{name}.zsync\0");
    fs::write(path, synthetic_elf(Some(info.as_bytes()))).unwrap();
}

fn saved_app(service: &Lifecycle, path: &Path) -> InstalledApp {
    Lifecycle::list_at(&service.library_path)
        .unwrap()
        .into_iter()
        .find(|app| app.file_path == path.to_str().unwrap())
        .unwrap()
}

fn save(service: &Lifecycle, path: &Path, change: impl FnOnce(&mut InstalledApp)) {
    persistence::mutate(&service.library_path, |library: &mut Library| {
        change(library.get_mut(path.to_str().unwrap()).unwrap());
        Ok(())
    })
    .unwrap();
}

#[test]
fn scanning_a_new_app_detects_its_update_source() {
    let (directory, service, source) = fixture();
    write_declaring(&source, "first");

    let report = service.scan(vec![directory.0.clone()]).unwrap();

    assert_eq!(report.apps[0].update_config, Some(zsync("first")));
    assert_eq!(
        report.apps[0].update_config_origin,
        Some(UpdateConfigOrigin::Detected)
    );
    assert_eq!(
        saved_app(&service, &source).update_config,
        Some(zsync("first"))
    );
}

#[test]
fn importing_a_new_app_detects_its_update_source() {
    let (_directory, service, source) = fixture();
    write_declaring(&source, "imported");

    let app = service.import_app(&source, false).unwrap();

    assert_eq!(app.update_config, Some(zsync("imported")));
    assert_eq!(app.update_config_origin, Some(UpdateConfigOrigin::Detected));
}

#[test]
fn apps_without_a_declared_source_stay_unconfigured() {
    let (directory, service, source) = fixture();

    let report = service.scan(vec![directory.0.clone()]).unwrap();

    assert_eq!(report.apps[0].update_config, None);
    assert_eq!(report.apps[0].update_config_origin, None);
    assert_eq!(saved_app(&service, &source).update_config_origin, None);
}

#[test]
fn rescanning_backfills_apps_saved_without_an_origin() {
    let (directory, service, source) = fixture();
    service.scan(vec![directory.0.clone()]).unwrap();
    write_declaring(&source, "backfilled");

    service.scan(vec![directory.0.clone()]).unwrap();

    let app = saved_app(&service, &source);
    assert_eq!(app.update_config, Some(zsync("backfilled")));
    assert_eq!(app.update_config_origin, Some(UpdateConfigOrigin::Detected));
}

#[test]
fn rescanning_never_detects_again_after_the_user_cleared_the_source() {
    let (directory, service, source) = fixture();
    service.scan(vec![directory.0.clone()]).unwrap();
    save(&service, &source, |app| app.set_user_update_config(None));
    write_declaring(&source, "ignored");

    service.scan(vec![directory.0.clone()]).unwrap();

    let app = saved_app(&service, &source);
    assert_eq!(app.update_config, None);
    assert_eq!(app.update_config_origin, Some(UpdateConfigOrigin::User));
}

#[test]
fn rescanning_keeps_a_source_the_user_chose() {
    let (directory, service, source) = fixture();
    write_declaring(&source, "first");
    service.scan(vec![directory.0.clone()]).unwrap();
    let chosen = UpdateConfig::DirectUrl {
        url: "https://example.org/mine.AppImage".into(),
    };
    save(&service, &source, |app| {
        app.set_user_update_config(Some(chosen.clone()))
    });
    write_declaring(&source, "second");

    service.scan(vec![directory.0.clone()]).unwrap();

    let app = saved_app(&service, &source);
    assert_eq!(app.update_config, Some(chosen));
    assert_eq!(app.update_config_origin, Some(UpdateConfigOrigin::User));
}

#[test]
fn rescanning_a_changed_file_refreshes_only_detected_sources() {
    let (directory, service, source) = fixture();
    write_declaring(&source, "first");
    service.scan(vec![directory.0.clone()]).unwrap();

    write_declaring(&source, "second-release");
    service.scan(vec![directory.0.clone()]).unwrap();
    assert_eq!(
        saved_app(&service, &source).update_config,
        Some(zsync("second-release"))
    );

    fs::write(&source, synthetic_elf(None)).unwrap();
    service.scan(vec![directory.0.clone()]).unwrap();
    let kept = saved_app(&service, &source);
    assert_eq!(kept.update_config, Some(zsync("second-release")));
    assert_eq!(
        kept.update_config_origin,
        Some(UpdateConfigOrigin::Detected)
    );
}

#[test]
fn user_saved_update_settings_record_their_origin() {
    let (directory, service, source) = fixture();
    write_declaring(&source, "first");
    service.scan(vec![directory.0.clone()]).unwrap();

    save(&service, &source, |app| {
        app.set_user_update_config(Some(zsync("manual")))
    });

    let app = saved_app(&service, &source);
    assert_eq!(app.update_config, Some(zsync("manual")));
    assert_eq!(app.update_config_origin, Some(UpdateConfigOrigin::User));
}

fn update_to(
    service: &Lifecycle,
    directory: &Path,
    app: &InstalledApp,
    staged: Vec<u8>,
) -> InstalledApp {
    let digest = crate::lifecycle::app_file::appimage_digest(Path::new(&app.file_path)).unwrap();
    let path = directory.join("download.tmp");
    fs::write(&path, staged).unwrap();
    service
        .replace_update(app, &path, "Example.AppImage", Some("2.0.0"), &digest)
        .unwrap()
}

#[test]
fn an_update_refreshes_a_detected_source_and_keeps_it_when_none_is_declared() {
    let (directory, service, source) = fixture();
    write_declaring(&source, "first");
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);

    let declared = update_to(
        &service,
        &directory.0,
        &app,
        synthetic_elf(Some(b"zsync|https://example.org/second.zsync\0")),
    );
    assert_eq!(declared.update_config, Some(zsync("second")));
    assert_eq!(
        declared.update_config_origin,
        Some(UpdateConfigOrigin::Detected)
    );

    let silent = update_to(&service, &directory.0, &declared, synthetic_elf(None));
    assert_eq!(silent.update_config, Some(zsync("second")));
    assert_eq!(
        silent.update_config_origin,
        Some(UpdateConfigOrigin::Detected)
    );
}

#[test]
fn an_update_keeps_a_source_the_user_chose() {
    let (directory, service, source) = fixture();
    write_declaring(&source, "first");
    service.scan(vec![directory.0.clone()]).unwrap();
    save(&service, &source, |app| {
        app.set_user_update_config(Some(zsync("manual")))
    });
    let app = saved_app(&service, &source);

    let updated = update_to(
        &service,
        &directory.0,
        &app,
        synthetic_elf(Some(b"zsync|https://example.org/second.zsync\0")),
    );

    assert_eq!(updated.update_config, Some(zsync("manual")));
    assert_eq!(updated.update_config_origin, Some(UpdateConfigOrigin::User));
}
