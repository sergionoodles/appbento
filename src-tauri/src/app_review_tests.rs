use super::fixture;
use crate::app_review::PreparedReviews;
use crate::models::{InstalledApp, LaunchConfig, LaunchMode};
use crate::persistence;
use crate::store::Library;
use std::fs;

fn direct_config() -> LaunchConfig {
    LaunchConfig {
        arguments: Vec::new(),
        environment: Default::default(),
        mode: LaunchMode::Direct,
        permissions: Default::default(),
    }
}

#[test]
fn initial_approval_requires_a_prepared_review() {
    let (directory, service, _) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);

    assert!(service
        .configure(&app.id, &direct_config(), None)
        .unwrap_err()
        .contains("Verify"));
    service.prepare_review(&app.id).unwrap();
    assert_eq!(service.list().unwrap()[0], app);
}

#[test]
fn prepared_approval_never_approves_later_file_contents() {
    let (directory, service, source) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let prepared = service.prepare_review(&app.id).unwrap();
    let approved_digest = prepared.digest_for(&app).unwrap();
    let mut bytes = fs::read(&source).unwrap();
    bytes.extend_from_slice(b"changed after verification");
    fs::write(&source, bytes).unwrap();

    let saved = service
        .configure(&app.id, &direct_config(), Some(&prepared))
        .unwrap();

    assert_eq!(
        saved.reviewed_digest.as_deref(),
        Some(approved_digest.as_str())
    );
    assert_ne!(
        saved.reviewed_digest,
        Some(crate::lifecycle::app_digest(&source, app.kind).unwrap())
    );
}

#[test]
fn ordinary_save_does_not_read_the_executable() {
    let (directory, service, source) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let config = direct_config();
    let review = service.prepare_review(&app.id).unwrap();
    let approved = service.configure(&app.id, &config, Some(&review)).unwrap();
    fs::remove_file(source).unwrap();
    let edited = LaunchConfig {
        arguments: vec!["--new-setting".into()],
        ..config
    };

    let saved = service.configure(&app.id, &edited, None).unwrap();

    assert_eq!(saved.reviewed_digest, approved.reviewed_digest);
    assert_eq!(saved.launch_config, Some(edited));
}

#[test]
fn stale_review_cannot_overwrite_a_newer_approval() {
    let (directory, service, _) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let old_review = service.prepare_review(&app.id).unwrap();
    let current_review = service.prepare_review(&app.id).unwrap();
    let approved = service
        .configure(&app.id, &direct_config(), Some(&current_review))
        .unwrap();

    assert!(service
        .configure(&app.id, &direct_config(), Some(&old_review))
        .unwrap_err()
        .contains("approval changed"));
    assert_eq!(service.list().unwrap()[0], approved);
}

#[test]
fn prepared_review_cannot_approve_another_app() {
    let (directory, service, source) = fixture();
    fs::copy(source, directory.0.join("Other.AppImage")).unwrap();
    let apps = service.scan(vec![directory.0.clone()]).unwrap().apps;
    let prepared = service.prepare_review(&apps[0].id).unwrap();

    assert!(service
        .configure(&apps[1].id, &direct_config(), Some(&prepared))
        .is_err());
    assert!(service
        .list()
        .unwrap()
        .iter()
        .all(|app| app.reviewed_digest.is_none()));
}

#[test]
fn failed_verification_and_invalidated_approval_do_not_grant_permissions() {
    let (directory, service, source) = fixture();
    let app = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let review = service.prepare_review(&app.id).unwrap();
    service
        .configure(&app.id, &direct_config(), Some(&review))
        .unwrap();
    persistence::mutate(&service.library_path, |library: &mut Library| {
        library.get_mut(&app.id).unwrap().reviewed_digest = None;
        Ok(())
    })
    .unwrap();
    fs::remove_file(source).unwrap();

    assert!(service.prepare_review(&app.id).is_err());
    assert!(service.configure(&app.id, &direct_config(), None).is_err());
    assert!(service.list().unwrap()[0].reviewed_digest.is_none());
}

#[test]
fn approval_tokens_are_consumed_and_unrecognized_tokens_are_rejected() {
    let (directory, service, _) = fixture();
    let app: InstalledApp = service
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    let reviews = PreparedReviews::default();
    let token = reviews
        .insert(service.prepare_review(&app.id).unwrap())
        .unwrap();
    let prepared = reviews.get(&token).unwrap();
    service
        .configure(&app.id, &direct_config(), Some(&prepared))
        .unwrap();
    reviews.remove(&token).unwrap();

    assert!(reviews.get(&token).is_err());
    assert!(reviews.get("not-a-token").is_err());
    assert!(reviews.get("999999").is_err());
}
