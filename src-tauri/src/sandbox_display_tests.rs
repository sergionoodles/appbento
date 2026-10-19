use super::*;

struct X11Socket {
    path: PathBuf,
    _listener: std::os::unix::net::UnixListener,
}

impl Drop for X11Socket {
    fn drop(&mut self) {
        fs::remove_file(&self.path).unwrap();
    }
}

#[test]
fn sandbox_shares_ipc_only_when_an_x11_socket_is_exposed() {
    let (directory, mut app) = fixture();
    let sockets = Path::new("/tmp/.X11-unix");
    fs::create_dir_all(sockets).unwrap();
    let display = format!(":{}", std::process::id());
    let socket_path = sockets.join(format!("X{}", std::process::id()));
    let _socket = X11Socket {
        _listener: std::os::unix::net::UnixListener::bind(&socket_path).unwrap(),
        path: socket_path,
    };
    let runtime = Runtime {
        x11_display: Some(display),
        directory: Some(directory.0.clone()),
        wayland_display: Some("wayland-test".into()),
        ..Default::default()
    };
    let _wayland_socket =
        std::os::unix::net::UnixListener::bind(directory.0.join("wayland-test")).unwrap();
    let can_run_bubblewrap = status().available;

    for (display_allowed, x11_display, network) in [
        (true, runtime.x11_display.clone(), false),
        (true, runtime.x11_display.clone(), true),
        (false, runtime.x11_display.clone(), false),
        (true, None, false),
        (true, Some(":invalid".into()), false),
        (true, Some(":99999999999999999999".into()), false),
        (true, Some("remote:0".into()), false),
    ] {
        let has_x11_access = display_allowed && x11_display == runtime.x11_display;
        app.launch_config = Some(LaunchConfig {
            arguments: Vec::new(),
            environment: Default::default(),
            mode: LaunchMode::Bubblewrap,
            permissions: SandboxPermissions {
                display: display_allowed,
                network,
                persistent_data: false,
                ..Default::default()
            },
        });
        let selected_runtime = Runtime {
            x11_display,
            directory: runtime.directory.clone(),
            wayland_display: runtime.wayland_display.clone(),
            ..Default::default()
        };
        let planned = plan(&app, &[], &directory.0.join("policy"), &selected_runtime).unwrap();
        let has_private_ipc = planned
            .arguments
            .iter()
            .any(|argument| argument == "--unshare-all" || argument == "--unshare-ipc");
        assert_eq!(has_private_ipc, !has_x11_access);
        assert_eq!(
            planned
                .arguments
                .windows(3)
                .any(|arguments| { arguments[0] == "--setenv" && arguments[1] == "DISPLAY" }),
            has_x11_access
        );
        assert_eq!(
            planned
                .arguments
                .iter()
                .any(|argument| argument == "--unshare-net"),
            !network
        );
        for flag in [
            "--unshare-user",
            "--unshare-pid",
            "--unshare-uts",
            "--unshare-cgroup-try",
        ] {
            assert!(planned.arguments.iter().any(|argument| argument == flag));
        }
        if can_run_bubblewrap {
            assert_ipc_namespace(planned, has_x11_access);
        }
    }
}

fn assert_ipc_namespace(mut planned: LaunchPlan, should_share_ipc: bool) {
    let host_ipc_namespace = fs::read_link("/proc/self/ns/ipc").unwrap();
    let command_index = planned
        .arguments
        .iter()
        .position(|argument| argument == "--")
        .unwrap();
    planned.arguments.truncate(command_index + 1);
    planned
        .arguments
        .extend(["/usr/bin/readlink".into(), "/proc/self/ns/ipc".into()]);
    let output = Command::new(&planned.program)
        .args(&planned.arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ipc_namespace = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        ipc_namespace.trim() == host_ipc_namespace.to_str().unwrap(),
        should_share_ipc
    );
}
