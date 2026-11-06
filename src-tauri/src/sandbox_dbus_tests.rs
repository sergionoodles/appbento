use super::*;
use crate::models::DbusRule;
use crate::persistence::tests::TemporaryDirectory;
use std::os::unix::net::UnixListener;

fn rule(name: &str, access: DbusAccess) -> DbusRule {
    DbusRule {
        name: name.into(),
        access,
    }
}

fn session(directory: &Path) -> DesktopSession {
    DesktopSession {
        app_id: "io.appbento.App.aexample".into(),
        instance_id: "appbento-0000000000000001".into(),
        runtime_directory: directory.to_owned(),
        bus_socket: directory.join("bus"),
        document_store: None,
    }
}

#[test]
fn app_ids_are_stable_valid_and_distinct_per_app() {
    // sha256("abc") starts with ba7816bf8f01cfea.
    assert_eq!(app_id("abc"), "io.appbento.App.aba7816bf8f01cfea");
    assert_ne!(app_id("/apps/one"), app_id("/apps/two"));
    let id = app_id("/apps/one");
    assert!(id.split('.').all(is_valid_element));
    assert_eq!(instance_id(0xdead_beef), "appbento-00000000deadbeef");
}

#[test]
fn each_enabled_option_maps_to_its_proxy_rule() {
    assert!(proxy_rules(&DesktopIntegration::default()).is_empty());
    assert_eq!(
        proxy_rules(&DesktopIntegration::recommended()),
        [
            "--talk=org.freedesktop.portal.*",
            "--talk=org.freedesktop.Notifications"
        ]
    );
    assert_eq!(
        proxy_rules(&DesktopIntegration {
            portals: true,
            notifications: true,
            tray: true,
            media_controls: true,
            secrets: true,
            custom: vec![
                rule("org.example.Service", DbusAccess::Talk),
                rule("org.example.Owned.*", DbusAccess::Own),
            ],
        }),
        [
            "--talk=org.freedesktop.portal.*",
            "--talk=org.freedesktop.Notifications",
            "--talk=org.kde.StatusNotifierWatcher",
            "--own=org.mpris.MediaPlayer2.*",
            "--talk=org.freedesktop.secrets",
            "--talk=org.example.Service",
            "--own=org.example.Owned.*",
        ]
    );
}

#[test]
fn custom_names_must_be_well_formed_and_never_expose_the_bus_itself() {
    let accepted = [
        "org.example.Service",
        "org.example.*",
        "a.b",
        "com.my-company.App_1",
        "org.example.x9",
    ];
    let rejected = [
        "",
        "single",
        "org.*.Service",
        "*",
        "org.example.**",
        "org..example",
        ".org.example",
        "org.example.",
        "org.1example.Service",
        "org.exa mple",
        "org.example=Service",
        "--talk=org.example",
        ":1.42",
        "org.freedesktop.DBus",
        "org.freedesktop.DBus.*",
        "org.freedesktop.*",
        "org.fréedesktop.Service",
    ];
    for name in accepted {
        let integration = DesktopIntegration {
            custom: vec![rule(name, DbusAccess::Talk)],
            ..Default::default()
        };
        assert_eq!(validate(&integration), Ok(()), "{name}");
    }
    for name in rejected {
        let integration = DesktopIntegration {
            custom: vec![rule(name, DbusAccess::Own)],
            ..Default::default()
        };
        assert!(
            validate(&integration).is_err(),
            "{name:?} should be rejected"
        );
    }
    let too_long = format!("org.{}", "a".repeat(MAX_BUS_NAME_LENGTH));
    assert!(validate_bus_name(&too_long).is_err());
}

#[test]
fn custom_rules_reject_host_execution_services_and_covering_wildcards() {
    for name in [
        "org.freedesktop.systemd1",
        "org.freedesktop.systemd1.*",
        "org.freedesktop.Flatpak",
        "org.kde.KWin",
        "org.kde.*",
        "org.gnome.Shell",
        "org.gnome.Shell.*",
        "org.gnome.*",
    ] {
        for access in [DbusAccess::Talk, DbusAccess::Own] {
            let integration = DesktopIntegration {
                custom: vec![rule(name, access)],
                ..Default::default()
            };
            assert!(validate(&integration).is_err(), "{name} must be blocked");
        }
    }
    assert!(validate_bus_name("org.kde.StatusNotifierWatcher").is_ok());
    assert!(validate_bus_name("org.gnome.ShellExtensions").is_ok());
}

#[test]
fn custom_rule_list_is_capped() {
    let rules = |count: usize| DesktopIntegration {
        custom: (0..count)
            .map(|index| rule(&format!("org.example.n{index}"), DbusAccess::Talk))
            .collect(),
        ..Default::default()
    };
    assert!(validate(&rules(MAX_CUSTOM_RULES)).is_ok());
    assert!(validate(&rules(MAX_CUSTOM_RULES + 1)).is_err());
}

#[test]
fn bus_addresses_yield_only_filesystem_sockets() {
    for (address, expected) in [
        ("unix:path=/run/user/1000/bus", Some("/run/user/1000/bus")),
        (
            "unix:path=/run/user/1000/bus,guid=abc123",
            Some("/run/user/1000/bus"),
        ),
        ("unix:guid=abc,path=/tmp/bus", Some("/tmp/bus")),
        ("unix:path=/tmp/with%20space", Some("/tmp/with space")),
        ("unix:abstract=/tmp/dbus-xyz,guid=abc", None),
        ("tcp:host=localhost,port=1234", None),
        ("unix:path=relative/bus", None),
        ("unix:path=/tmp/bad%zz", None),
        ("", None),
        (
            "unix:abstract=/tmp/dbus-xyz;unix:path=/run/user/1000/bus",
            Some("/run/user/1000/bus"),
        ),
    ] {
        assert_eq!(
            bus_socket_from_address(address),
            expected.map(PathBuf::from),
            "{address}"
        );
    }
}

#[test]
fn bus_resolution_prefers_the_address_and_falls_back_to_the_runtime_socket() {
    let directory = TemporaryDirectory::new();
    let runtime_socket = directory.0.join("bus");
    let other_socket = directory.0.join("other");
    assert!(resolve_bus_socket(None, &directory.0).is_err());

    let _runtime_listener = UnixListener::bind(&runtime_socket).unwrap();
    let _other_listener = UnixListener::bind(&other_socket).unwrap();
    assert_eq!(
        resolve_bus_socket(None, &directory.0),
        Ok(runtime_socket.clone())
    );
    assert_eq!(
        resolve_bus_socket(Some(""), &directory.0),
        Ok(runtime_socket)
    );
    assert_eq!(
        resolve_bus_socket(
            Some(&format!("unix:path={},guid=1", other_socket.display())),
            &directory.0
        ),
        Ok(other_socket)
    );

    for address in [
        "unix:abstract=/tmp/dbus-xyz",
        "unix:path=/nonexistent/bus",
        &format!("unix:path={}", directory.0.display()),
    ] {
        let error = resolve_bus_socket(Some(address), &directory.0).unwrap_err();
        assert!(error.contains("Turn off Desktop integration"), "{error}");
    }
}

#[test]
fn session_paths_stay_inside_this_instance() {
    let session = session(Path::new("/run/user/1000"));
    assert_eq!(
        session.instance_directory(),
        Path::new("/run/user/1000/.flatpak/appbento-0000000000000001")
    );
    assert_eq!(
        session.proxy_socket(),
        Path::new("/run/user/1000/appbento/dbus/appbento-0000000000000001/bus")
    );
    assert_eq!(
        session.keyfile(),
        "[Application]\nname=io.appbento.App.aexample\n\n[Instance]\ninstance-id=appbento-0000000000000001\n"
    );
}

#[test]
fn app_arguments_are_empty_when_integration_is_off() {
    let mut arguments = Vec::new();
    add_app_arguments(&mut arguments, &SandboxPermissions::default(), None).unwrap();
    assert!(arguments.is_empty());
}

#[test]
fn app_arguments_require_a_session_when_integration_is_on() {
    let permissions = SandboxPermissions {
        desktop_integration: DesktopIntegration::recommended(),
        ..Default::default()
    };
    let error = add_app_arguments(&mut Vec::new(), &permissions, None).unwrap_err();
    assert!(error.contains("session bus"), "{error}");
}

#[test]
fn app_arguments_wire_the_proxied_bus_and_portal_identity() {
    let permissions = SandboxPermissions {
        desktop_integration: DesktopIntegration::recommended(),
        ..Default::default()
    };
    let mut session = session(Path::new("/run/user/1000"));
    let mut arguments = Vec::new();
    add_app_arguments(&mut arguments, &permissions, Some(&session)).unwrap();
    assert_eq!(
        arguments,
        [
            "--info-fd",
            BWRAP_INFO_PLACEHOLDER,
            "--ro-bind-data",
            APP_INFO_PLACEHOLDER,
            "/.flatpak-info",
            "--bind",
            "/run/user/1000/appbento/dbus/appbento-0000000000000001/bus",
            "/run/user/app/bus",
            "--setenv",
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/run/user/app/bus",
        ]
    );

    session.document_store = Some("/run/user/1000/doc/by-app/io.appbento.App.aexample".into());
    let mut with_documents = Vec::new();
    add_app_arguments(&mut with_documents, &permissions, Some(&session)).unwrap();
    assert_eq!(
        with_documents[arguments.len()..],
        [
            "--bind",
            "/run/user/1000/doc/by-app/io.appbento.App.aexample",
            "/run/user/1000/doc"
        ]
    );
}

#[test]
fn proxy_runs_in_its_own_sandbox_with_only_the_filter_rules() {
    let session = session(Path::new("/run/user/1000"));
    let rules = vec!["--talk=org.freedesktop.portal.*".to_owned()];
    let arguments = proxy_arguments(&session, "/usr/bin/xdg-dbus-proxy", &rules, 7, 9).unwrap();
    let separator = arguments
        .iter()
        .position(|argument| argument == "--")
        .unwrap();
    assert_eq!(
        arguments[separator + 1..],
        [
            "/usr/bin/xdg-dbus-proxy",
            "unix:path=/run/user/1000/bus",
            "/run/user/1000/appbento/dbus/appbento-0000000000000001/bus",
            "--filter",
            "--fd=9",
            "--talk=org.freedesktop.portal.*",
        ]
    );
    let sandbox = &arguments[..separator];
    for expected in [
        ["--ro-bind-data", "7", "/.flatpak-info"],
        ["--bind", "/run/user/1000/bus", "/run/user/1000/bus"],
    ] {
        assert!(
            sandbox.windows(3).any(|window| window == expected),
            "{expected:?}"
        );
    }
    for option in [
        "--die-with-parent",
        "--unshare-user",
        "--unshare-pid",
        "--unshare-net",
    ] {
        assert!(sandbox.contains(&option.to_owned()), "{option}");
    }
    assert!(!sandbox.contains(&"--share-net".to_owned()));
}

#[test]
fn proxy_sandbox_mirrors_how_the_host_lays_out_usr_links() {
    let root = TemporaryDirectory::new();
    fs::create_dir(root.0.join("usr")).unwrap();
    std::os::unix::fs::symlink("usr/bin", root.0.join("bin")).unwrap();
    fs::create_dir(root.0.join("lib")).unwrap();
    let mut arguments = Vec::new();
    add_system_mounts(&mut arguments, &root.0);
    assert_eq!(
        arguments,
        [
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--ro-bind",
            "/lib",
            "/lib",
        ]
    );
}
