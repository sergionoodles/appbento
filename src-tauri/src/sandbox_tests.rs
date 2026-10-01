use super::*;
use crate::models::{PathPermission, SandboxPermissions};
use crate::persistence::tests::TemporaryDirectory;

fn fixture() -> (TemporaryDirectory, InstalledApp) {
    let directory = TemporaryDirectory::new();
    let image = directory.0.join("Example.AppImage");
    let mut header = [0; 20];
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
    fs::write(&image, header).unwrap();
    let lifecycle = crate::lifecycle::Lifecycle {
        library_path: directory.0.join("policy/library.json"),
        managed_dir: directory.0.join("managed"),
        desktop_dir: directory.0.join("desktop"),
        catalog: Vec::new(),
    };
    let app = lifecycle
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    (directory, app)
}

#[test]
fn unreviewed_app_cannot_launch_and_scan_does_not_apply_permissions() {
    let (directory, app) = fixture();
    assert!(app.launch_config.is_none());
    assert!(
        plan(&app, &[], &directory.0.join("policy"), &Runtime::default())
            .unwrap_err()
            .contains("Review")
    );
}

#[test]
fn default_plan_denies_host_access_and_preserves_uri_arguments() {
    let (directory, mut app) = fixture();
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        permissions: SandboxPermissions::default(),
    });
    let plan = plan(
        &app,
        &["https://example.org".into()],
        &directory.0.join("policy"),
        &Runtime::default(),
    )
    .unwrap();
    assert_eq!(plan.program, bwrap_program());
    assert!(plan.clear_environment);
    for denied in [
        "--share-net",
        "/dev/dri",
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "PULSE_SERVER",
        "XAUTHORITY",
    ] {
        assert!(!plan.arguments.iter().any(|argument| argument == denied));
    }
    assert!(plan.arguments.contains(&"--unshare-user".into()));
    assert!(plan.arguments.contains(&"--clearenv".into()));
    assert!(plan.arguments.ends_with(&[
        "--appimage-extract-and-run".into(),
        "https://example.org".into()
    ]));
}

#[test]
fn grants_are_explicit_and_protected_paths_and_traversal_are_rejected() {
    let (directory, mut app) = fixture();
    let documents = directory.0.join("documents");
    fs::create_dir(&documents).unwrap();
    let permissions = SandboxPermissions {
        network: true,
        paths: vec![PathPermission {
            path: documents.to_str().unwrap().into(),
            access: PathAccess::ReadOnly,
        }],
        ..Default::default()
    };
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        permissions,
    });
    let policy = directory.0.join("policy");
    let planned = plan(&app, &[], &policy, &Runtime::default()).unwrap();
    assert!(planned.arguments.contains(&"--share-net".into()));
    assert!(planned.arguments.windows(3).any(|arguments| arguments
        == [
            "--ro-bind",
            documents.to_str().unwrap(),
            documents.to_str().unwrap()
        ]));
    for denied in [
        "/",
        "/run",
        "/dev",
        "/usr",
        "/tmp",
        directory.0.to_str().unwrap(),
        "/tmp/../tmp",
        "relative",
    ] {
        assert!(
            validate_grant(Path::new(denied), &policy).is_err(),
            "{denied}"
        );
    }
    let link = directory.0.join("link");
    std::os::unix::fs::symlink(&policy, &link).unwrap();
    assert!(validate_grant(&link, &policy).is_err());
}

#[test]
fn actual_bubblewrap_isolates_home_files_environment_and_network() {
    let status = status();
    if !status.available {
        eprintln!("Bubblewrap smoke test skipped: {:?}", status.error);
        return;
    }
    let (directory, mut app) = fixture();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let secret = directory.0.join("host-secret");
    fs::write(&secret, "private").unwrap();
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        permissions: SandboxPermissions {
            persistent_data: false,
            ..Default::default()
        },
    });
    let mut planned = plan(&app, &[], &directory.0.join("policy"), &Runtime::default()).unwrap();
    let command_index = planned
        .arguments
        .iter()
        .position(|argument| argument == "--")
        .unwrap();
    planned.arguments.truncate(command_index + 1);
    planned.arguments.extend(["/bin/sh".into(), "-c".into(), format!("test ! -e '{}' && test -z \"$APPBENTO_HOST_SECRET\" && test \"$HOME\" = /home/app && test ! -e /run/dbus/system_bus_socket && test ! -e /dev/fuse && ! /usr/bin/timeout 1 /bin/bash -c 'echo >/dev/tcp/127.0.0.1/{port}'", secret.display())]);
    let output = Command::new(planned.program)
        .args(planned.arguments)
        .env("APPBENTO_HOST_SECRET", "secret")
        .env_clear()
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn catalog_profiles_are_suggestions_and_reject_unknown_fields() {
    let entries = crate::catalog::load_catalog().unwrap();
    assert!(entries.iter().any(|entry| entry.sandbox.is_some()));
    assert!(serde_json::from_str::<SandboxPermissions>("{\"unrestricted\":true}").is_err());
    let permissions: SandboxPermissions = serde_json::from_str("{}").unwrap();
    assert!(permissions.display && permissions.persistent_data);
    assert!(!permissions.network && !permissions.audio && !permissions.gpu);
    assert!(permissions.paths.is_empty());
}

#[test]
fn folder_and_image_replacement_after_planning_preserves_reviewed_access() {
    if !status().available {
        return;
    }
    let (directory, mut app) = fixture();
    let policy = directory.0.join("policy");
    fs::write(policy.join("secret"), "private").unwrap();
    let approved = directory.0.join("approved");
    fs::create_dir(&approved).unwrap();
    fs::write(approved.join("allowed"), "public").unwrap();
    app.reviewed_digest = Some(appimage_digest(Path::new(&app.file_path)).unwrap());
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        permissions: SandboxPermissions {
            persistent_data: false,
            paths: vec![PathPermission {
                path: approved.to_str().unwrap().into(),
                access: PathAccess::ReadWrite,
            }],
            ..Default::default()
        },
    });
    let mut planned = plan(&app, &[], &policy, &Runtime::default()).unwrap();
    let pinned = execution::pin_launch_paths_in_place(&mut planned, &app, &policy).unwrap();
    let mut changed_image = fs::read(&app.file_path).unwrap();
    changed_image.extend_from_slice(b"unreviewed application bytes");
    fs::write(&app.file_path, changed_image).unwrap();
    fs::rename(&approved, directory.0.join("original-approved")).unwrap();
    std::os::unix::fs::symlink(&policy, &approved).unwrap();
    let command_index = planned
        .arguments
        .iter()
        .position(|argument| argument == "--")
        .unwrap();
    planned.arguments.truncate(command_index + 1);
    planned.arguments.extend([
        "/bin/sh".into(),
        "-c".into(),
        format!(
            "test -e '{}/allowed' && test ! -e '{}/secret' && test $(wc -c </app/application.AppImage) -eq 20",
            approved.display(),
            approved.display()
        ),
    ]);
    let mut command = Command::new(planned.program);
    command.args(planned.arguments).env_clear();
    execution::inherit_descriptors(&mut command, &pinned);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn direct_stdin_worker() {
    let Some(image) = std::env::var_os("APPBENTO_TEST_DIRECT_IMAGE") else {
        return;
    };
    let planned = LaunchPlan {
        program: image.to_string_lossy().into_owned(),
        arguments: Vec::new(),
        clear_environment: false,
    };
    let output = execution::prepare_command(&planned).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"direct-input\n");
}

#[test]
fn direct_launch_preserves_inherited_standard_input() {
    use std::io::Write;
    let directory = TemporaryDirectory::new();
    let image = directory.0.join("cat.AppImage");
    let mut bytes = fs::read("/usr/bin/cat").unwrap();
    bytes[8..11].copy_from_slice(b"AI\x02");
    fs::write(&image, bytes).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&image, fs::Permissions::from_mode(0o700)).unwrap();
    let mut worker = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "sandbox::tests::direct_stdin_worker"])
        .env("APPBENTO_TEST_DIRECT_IMAGE", &image)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    worker
        .stdin
        .take()
        .unwrap()
        .write_all(b"direct-input\n")
        .unwrap();
    assert!(worker.wait().unwrap().success());
}

#[test]
fn changed_intermediate_folder_symlink_requires_review_instead_of_unpinned_mount() {
    let (directory, mut app) = fixture();
    let approved = directory.0.join("approved");
    let alternative = directory.0.join("alternative");
    fs::create_dir_all(approved.join("nested")).unwrap();
    fs::create_dir_all(alternative.join("nested")).unwrap();
    let grant = approved.join("nested");
    app.reviewed_digest = Some(appimage_digest(Path::new(&app.file_path)).unwrap());
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        permissions: SandboxPermissions {
            persistent_data: false,
            paths: vec![PathPermission {
                path: grant.to_str().unwrap().into(),
                access: PathAccess::ReadWrite,
            }],
            ..Default::default()
        },
    });
    fs::rename(&approved, directory.0.join("original-approved")).unwrap();
    std::os::unix::fs::symlink(&alternative, &approved).unwrap();
    let policy = directory.0.join("policy");
    let mut planned = plan(&app, &[], &policy, &Runtime::default()).unwrap();
    assert!(execution::pin_launch_paths_in_place(&mut planned, &app, &policy).is_err());
}
