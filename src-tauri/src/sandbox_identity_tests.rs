use super::*;

#[test]
fn restricted_plan_has_an_explicit_account_identity() {
    let (directory, mut app) = tests::fixture();
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        arguments: Vec::new(),
        environment: Default::default(),
        permissions: SandboxPermissions::default(),
    });
    let planned = plan(&app, &[], &directory.0.join("policy"), &Runtime::default()).unwrap();
    assert!(planned
        .arguments
        .windows(2)
        .any(|pair| pair == ["--uid", "1000"]));
    assert!(planned
        .arguments
        .windows(3)
        .any(|triple| triple == ["--setenv", "USER", "app"]));
}

#[test]
fn injected_identity_and_root_policy() {
    let (directory, mut app) = tests::fixture();
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        arguments: vec![],
        environment: Default::default(),
        permissions: SandboxPermissions::default(),
    });
    let mut runtime = Runtime {
        identity: Identity {
            uid: 23456,
            gid: 34567,
        },
        ..Runtime::default()
    };
    let planned = plan(&app, &[], &directory.0.join("policy"), &runtime).unwrap();
    assert!(planned
        .arguments
        .windows(2)
        .any(|pair| pair == ["--uid", "23456"]));
    assert!(planned
        .arguments
        .windows(2)
        .any(|pair| pair == ["--gid", "34567"]));
    runtime.identity.uid = 0;
    assert!(plan(&app, &[], &directory.0.join("policy"), &runtime)
        .unwrap_err()
        .contains("nonroot"));
    app.launch_config.as_mut().unwrap().mode = LaunchMode::Direct;
    assert!(plan(&app, &[], &directory.0.join("policy"), &runtime).is_ok());
}

#[test]
fn account_descriptors_are_minimal_sealed_and_closed_on_exec() {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::MetadataExt;
    let files = Identity {
        uid: 23456,
        gid: 34567,
    }
    .files()
    .unwrap();
    for (mut file, destination) in files {
        let descriptor = file.as_raw_fd();
        let inode = file.metadata().unwrap().ino();
        let mut data = String::new();
        file.read_to_string(&mut data).unwrap();
        let expected = if destination == "/etc/passwd" {
            "app:x:23456:34567::/home/app:/bin/sh\n"
        } else {
            "app:x:34567:\n"
        };
        assert_eq!(data, expected);
        assert!(file.write_all(b"host-account").is_err());
        let output = Command::new("/bin/sh")
            .args(["-c", &format!("test ! -e /proc/self/fd/{descriptor}")])
            .status()
            .unwrap();
        assert!(output.success());
        drop(file);
        assert!(!fs::metadata(format!("/proc/self/fd/{descriptor}"))
            .is_ok_and(|metadata| metadata.ino() == inode));
    }
}

#[test]
fn actual_account_lookup_and_descriptor_consumption() {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;
    unsafe extern "C" {
        fn dup(descriptor: i32) -> i32;
    }
    if !status().available {
        return;
    }
    let (directory, mut app) = tests::fixture();
    let policy = directory.0.join("policy");
    let home = private_home_path(&policy, &app.id);
    fs::create_dir_all(&home).unwrap();
    let shared = directory.0.join("shared");
    fs::create_dir(&shared).unwrap();
    let private_path = policy.join("unshared-test-data");
    fs::write(&private_path, "host-only-test-value").unwrap();
    let private_file = fs::File::open(private_path).unwrap();
    app.reviewed_digest = Some(appimage_digest(Path::new(&app.file_path)).unwrap());
    for (network, writable_accounts) in [(false, false), (true, false), (false, true)] {
        app.launch_config = Some(LaunchConfig {
            mode: LaunchMode::Bubblewrap,
            arguments: vec![],
            environment: Default::default(),
            permissions: SandboxPermissions {
                network,
                display: network,
                persistent_data: true,
                paths: vec![PathPermission {
                    path: shared.to_string_lossy().into(),
                    access: PathAccess::ReadWrite,
                }],
                ..Default::default()
            },
        });
        let runtime = Runtime {
            identity: Identity {
                uid: 23456,
                gid: 34567,
            },
            ..Runtime::default()
        };
        let mut planned = plan(&app, &[], &policy, &runtime).unwrap();
        let pinned = execution::pin_launch_paths_in_place(&mut planned, &app, &policy).unwrap();
        let index = planned
            .arguments
            .iter()
            .position(|argument| argument == "--")
            .unwrap();
        planned.arguments.truncate(index);
        if writable_accounts {
            for option in planned.arguments[..6].chunks_exact_mut(3) {
                assert_eq!(option[0], "--ro-bind-data");
                option[0] = "--bind-data".into();
            }
        }
        planned.arguments.push("--".into());
        planned.arguments.extend([
            "/bin/sh".into(),
            "-c".into(),
            r#"
set -eux
test "$(id -u)" = 23456
test "$(id -g)" = 34567
test "$(getent passwd "$(id -u)")" = 'app:x:23456:34567::/home/app:/bin/sh'
test "$(getent group "$(id -g)")" = 'app:x:34567:'
test "$(wc -l </etc/passwd)" = 1
test "$(wc -l </etc/group)" = 1
test "$USER:$LOGNAME:$HOME" = app:app:/home/app
test ! -e /etc/shadow
test ! -e /etc/gshadow
test ! -e /run/dbus/system_bus_socket
if (echo leak >>/etc/passwd) 2>/dev/null; then
    echo 'Sandbox passwd file is writable' >&2
    exit 8
fi
if (echo leak >>/etc/group) 2>/dev/null; then
    echo 'Sandbox group file is writable' >&2
    exit 8
fi
for fd in /proc/self/fd/*; do
    number=${fd##*/}
    if test "$number" -gt 2 && test -e "$fd"; then
        echo "Unexpected inherited descriptor $number: $(readlink "$fd")" >&2
        exit 9
    fi
done
"#
            .into(),
        ]);
        let mut command = execution::prepare_command(&planned);
        let private_descriptor = private_file.as_raw_fd();
        // Simulate a library opening a descriptor without CLOEXEC. Do this only
        // after fork, so other concurrently running tests cannot inherit it.
        unsafe {
            command.pre_exec(move || {
                if dup(private_descriptor) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        execution::inherit_descriptors(&mut command, &planned, &pinned);
        let output = command.output().unwrap();
        assert!(
            output.status.success() != writable_accounts,
            "status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        if writable_accounts {
            assert_eq!(output.status.code(), Some(8));
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("Sandbox passwd file is writable")
            );
        }
    }
}

#[test]
fn pinned_account_lifetime_and_user_values_do_not_affect_mount_selection() {
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;
    let (directory, mut app) = tests::fixture();
    let policy = directory.0.join("policy");
    let shared = directory.0.join("shared");
    fs::create_dir(&shared).unwrap();
    app.reviewed_digest = Some(appimage_digest(Path::new(&app.file_path)).unwrap());
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        arguments: vec![
            "--ro-bind".into(),
            shared.to_string_lossy().into(),
            shared.to_string_lossy().into(),
        ],
        environment: [("DELIMITER".into(), "--".into())].into(),
        permissions: SandboxPermissions {
            persistent_data: false,
            paths: vec![PathPermission {
                path: shared.to_string_lossy().into(),
                access: PathAccess::ReadOnly,
            }],
            ..Default::default()
        },
    });
    let mut planned = plan(&app, &[], &policy, &Runtime::default()).unwrap();
    let original_command = planned.arguments[planned.arguments.len() - 3..].to_vec();
    let pinned = execution::pin_launch_paths_in_place(&mut planned, &app, &policy).unwrap();
    assert!(planned.arguments.ends_with(&original_command));
    let descriptor_paths = planned.arguments[..6]
        .chunks_exact(3)
        .map(|option| {
            assert_eq!(option[0], "--ro-bind-data");
            let path = format!("/proc/self/fd/{}", option[1]);
            let mut data = String::new();
            std::fs::File::open(&path)
                .unwrap()
                .read_to_string(&mut data)
                .unwrap();
            assert!(data.starts_with("app:x:1000:"));
            let inode = fs::metadata(&path).unwrap().ino();
            (path, inode)
        })
        .collect::<Vec<_>>();
    drop(pinned);
    for (path, inode) in descriptor_paths {
        assert!(!fs::metadata(&path).is_ok_and(|metadata| metadata.ino() == inode));
    }
}

#[test]
fn pinning_accepts_a_plan_with_all_permission_types() {
    use std::os::unix::net::UnixListener;
    let (directory, mut app) = tests::fixture();
    let policy = directory.0.join("policy");
    fs::create_dir_all(private_home_path(&policy, &app.id)).unwrap();
    let shared = directory.0.join("shared");
    fs::create_dir(&shared).unwrap();
    fs::create_dir(directory.0.join("pulse")).unwrap();
    let _audio = UnixListener::bind(directory.0.join("pulse/native")).unwrap();
    let _wayland = UnixListener::bind(directory.0.join("wayland-test")).unwrap();
    let runtime = Runtime {
        directory: Some(directory.0.clone()),
        wayland_display: Some("wayland-test".into()),
        desktop_session: Some(DesktopSession {
            app_id: "io.appbento.App.atest".into(),
            instance_id: "appbento-test".into(),
            runtime_directory: directory.0.clone(),
            bus_socket: directory.0.join("bus"),
            document_store: None,
        }),
        ..Runtime::default()
    };
    app.reviewed_digest = Some(appimage_digest(Path::new(&app.file_path)).unwrap());
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        arguments: Vec::new(),
        environment: Default::default(),
        permissions: SandboxPermissions {
            network: true,
            display: true,
            audio: true,
            gpu: true,
            persistent_data: true,
            paths: vec![PathPermission {
                path: shared.to_string_lossy().into(),
                access: PathAccess::ReadOnly,
            }],
            desktop_integration: DesktopIntegration::recommended(),
        },
    });
    let mut planned = plan(&app, &[], &policy, &runtime).unwrap();
    let _pinned = execution::pin_launch_paths_in_place(&mut planned, &app, &policy).unwrap();
}

#[test]
fn pinning_rejects_unknown_and_incomplete_options() {
    let (directory, mut app) = tests::fixture();
    let policy = directory.0.join("policy");
    app.reviewed_digest = Some(appimage_digest(Path::new(&app.file_path)).unwrap());
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        arguments: Vec::new(),
        environment: Default::default(),
        permissions: SandboxPermissions::default(),
    });
    for (arguments, message) in [
        (
            vec!["--unexpected-option", "--"],
            "unsupported sandbox option",
        ),
        (vec!["--bind", "/tmp"], "incomplete sandbox option"),
    ] {
        let mut planned = plan(&app, &[], &policy, &Runtime::default()).unwrap();
        planned.arguments = arguments.into_iter().map(str::to_owned).collect();
        let result = execution::pin_launch_paths_in_place(&mut planned, &app, &policy);
        assert!(result.err().unwrap().contains(message));
    }
}
