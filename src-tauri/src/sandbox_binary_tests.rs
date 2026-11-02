use super::*;
use crate::lifecycle::executable_tests::{binary_bytes, binary_fixture, bytes_with_loader};

fn reviewed_binary() -> (crate::persistence::tests::TemporaryDirectory, InstalledApp) {
    let (directory, service, path) = binary_fixture();
    let app = service.import_app(&path, false).unwrap();
    let config = LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        arguments: vec!["--profile".into(), "a profile".into()],
        environment: [("LABEL".into(), "a value".into())].into(),
        permissions: SandboxPermissions {
            display: false,
            persistent_data: false,
            ..default_permissions(&app, &directory.0.join("policy"))
        },
    };
    let app = service.configure(&app.id, &config).unwrap();
    (directory, app)
}

#[test]
fn default_permissions_grant_only_valid_binary_parent_folders() {
    let (directory, mut app) = reviewed_binary();
    let policy = directory.0.join("policy");
    let permissions = default_permissions(&app, &policy);
    assert_eq!(
        permissions.paths,
        vec![PathPermission {
            path: directory.0.join("bundle").to_string_lossy().into_owned(),
            access: PathAccess::ReadOnly
        }]
    );
    app.kind = AppKind::AppImage;
    assert_eq!(
        default_permissions(&app, &policy),
        SandboxPermissions::default()
    );
    app.kind = AppKind::Binary;
    for parent in [&directory.0, Path::new("/usr/local/bin"), Path::new("/app")] {
        app.file_path = parent.join("app").to_string_lossy().into_owned();
        assert!(
            default_permissions(&app, &directory.0.join("appbento-policy"))
                .paths
                .is_empty()
        );
    }
    if let Some(home) = dirs::home_dir() {
        app.file_path = home.join("app").to_string_lossy().into_owned();
        assert!(
            default_permissions(&app, &home.join(".local/share/appbento"))
                .paths
                .is_empty()
        );
    }
}

#[test]
fn binary_mounts_keep_filename_and_overlay_grants_before_literal_arguments() {
    let (directory, mut app) = reviewed_binary();
    let policy = directory.0.join("policy");
    for has_grant in [true, false] {
        if !has_grant {
            app.launch_config
                .as_mut()
                .unwrap()
                .permissions
                .paths
                .clear();
        }
        let planned = plan(&app, &["uri argument".into()], &policy, &Runtime::default()).unwrap();
        let destination = if has_grant {
            app.file_path.clone()
        } else {
            format!("/app/{}", app.file_name)
        };
        assert_eq!(
            planned.application_destination.as_deref(),
            Some(destination.as_str())
        );
        assert!(planned.arguments.ends_with(&[
            destination.clone(),
            "--profile".into(),
            "a profile".into(),
            "uri argument".into()
        ]));
        assert!(!planned
            .arguments
            .iter()
            .any(|argument| argument == "--appimage-extract-and-run"));
        assert!(planned
            .arguments
            .windows(3)
            .any(|args| args == ["--setenv", "LABEL", "a value"]));
        let app_index = planned
            .arguments
            .windows(3)
            .position(|args| args == ["--ro-bind", &app.file_path, &destination])
            .unwrap();
        if has_grant {
            let parent = Path::new(&app.file_path)
                .parent()
                .unwrap()
                .to_str()
                .unwrap();
            let grant_index = planned
                .arguments
                .windows(3)
                .position(|args| args == ["--ro-bind", parent, parent])
                .unwrap();
            assert!(app_index > grant_index);
        }
    }
    app.launch_config.as_mut().unwrap().mode = LaunchMode::Direct;
    let direct = plan(&app, &[], &policy, &Runtime::default()).unwrap();
    assert_eq!(direct.program, app.file_path);
    assert!(direct.application_destination.is_none());
}

#[test]
fn binary_without_review_cannot_plan_and_unmounted_loader_has_a_specific_error() {
    let (directory, service, path) = binary_fixture();
    let mut app = service.import_app(&path, false).unwrap();
    assert!(
        plan(&app, &[], &directory.0.join("policy"), &Runtime::default())
            .unwrap_err()
            .contains("Review")
    );
    let loader_folder = directory.0.join("loader");
    fs::create_dir(&loader_folder).unwrap();
    let loader = loader_folder.join("ld.so");
    fs::write(&loader, b"loader fixture").unwrap();
    fs::write(
        &path,
        bytes_with_loader(format!("{}\0", loader.display()).as_bytes()),
    )
    .unwrap();
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        arguments: vec![],
        environment: Default::default(),
        permissions: SandboxPermissions {
            display: false,
            ..Default::default()
        },
    });
    let error = plan(&app, &[], &directory.0.join("policy"), &Runtime::default()).unwrap_err();
    assert!(
        error.contains("Program loader") && error.contains("outside the mounted"),
        "{error}"
    );
    app.launch_config.as_mut().unwrap().permissions.paths = vec![PathPermission {
        path: loader_folder.to_string_lossy().into_owned(),
        access: PathAccess::ReadOnly,
    }];
    assert!(plan(&app, &[], &directory.0.join("policy"), &Runtime::default()).is_ok());
}

#[test]
fn binary_snapshot_pins_correct_destination_and_cleans_on_success_and_mismatch() {
    let (directory, mut app) = reviewed_binary();
    let policy = directory.0.join("snapshots");
    fs::create_dir(&policy).unwrap();
    for has_grant in [true, false] {
        if !has_grant {
            app.launch_config
                .as_mut()
                .unwrap()
                .permissions
                .paths
                .clear();
        }
        let mut planned = plan(&app, &[], &policy, &Runtime::default()).unwrap();
        let destination = planned.application_destination.clone().unwrap();
        let pinned = execution::pin_launch_paths_in_place(&mut planned, &app, &policy).unwrap();
        let mounts = planned
            .arguments
            .windows(3)
            .filter(|args| args[0] == "--ro-bind-fd")
            .collect::<Vec<_>>();
        assert_eq!(mounts.len(), if has_grant { 2 } else { 1 });
        assert!(mounts.iter().any(|args| args[2] == destination));
        assert_eq!(fs::read_dir(&policy).unwrap().count(), 1);
        drop(pinned);
        assert_eq!(fs::read_dir(&policy).unwrap().count(), 0);
    }
    let mut planned = plan(&app, &[], &policy, &Runtime::default()).unwrap();
    let mut changed = binary_bytes();
    changed.push(42);
    fs::write(&app.file_path, changed).unwrap();
    let result = execution::pin_launch_paths_in_place(&mut planned, &app, &policy);
    assert!(result.err().unwrap().contains("changed since review"));
    assert_eq!(fs::read_dir(&policy).unwrap().count(), 0);
    fs::write(&app.file_path, b"invalid").unwrap();
    assert!(execution::pin_launch_paths_in_place(&mut planned, &app, &policy).is_err());
    assert_eq!(fs::read_dir(&policy).unwrap().count(), 0);
}
