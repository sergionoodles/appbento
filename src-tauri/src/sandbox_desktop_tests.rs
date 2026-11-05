use super::*;
use crate::models::DesktopIntegration;

fn bubblewrap_config(desktop_integration: DesktopIntegration) -> LaunchConfig {
    LaunchConfig {
        arguments: Vec::new(),
        environment: Default::default(),
        mode: LaunchMode::Bubblewrap,
        permissions: SandboxPermissions {
            persistent_data: false,
            desktop_integration,
            ..Default::default()
        },
    }
}

fn desktop_runtime(directory: &Path, document_store: bool) -> Runtime {
    Runtime {
        desktop_session: Some(DesktopSession {
            app_id: "io.appbento.App.aexample".into(),
            instance_id: "appbento-0000000000000001".into(),
            runtime_directory: directory.to_owned(),
            bus_socket: directory.join("bus"),
            document_store: document_store.then(|| directory.join("doc/by-app/example")),
        }),
        ..Default::default()
    }
}

#[test]
fn plan_without_desktop_integration_never_mentions_the_bus() {
    let (directory, mut app) = fixture();
    app.launch_config = Some(bubblewrap_config(DesktopIntegration::default()));
    let policy = directory.0.join("policy");
    let baseline = plan(&app, &[], &policy, &Runtime::default()).unwrap();
    // A prepared session is ignored when the app did not ask for integration.
    let with_session = plan(&app, &[], &policy, &desktop_runtime(&directory.0, true)).unwrap();
    assert_eq!(baseline.arguments, with_session.arguments);
    for denied in [
        "--info-fd",
        "/.flatpak-info",
        "DBUS_SESSION_BUS_ADDRESS",
        "/run/user/app/bus",
    ] {
        assert!(!baseline.arguments.iter().any(|argument| argument == denied));
    }
}

#[test]
fn plan_with_desktop_integration_adds_the_bus_identity_and_documents() {
    let (directory, mut app) = fixture();
    app.launch_config = Some(bubblewrap_config(DesktopIntegration::recommended()));
    let planned = plan(
        &app,
        &[],
        &directory.0.join("policy"),
        &desktop_runtime(&directory.0, true),
    )
    .unwrap();
    let has = |expected: &[&str]| {
        planned
            .arguments
            .windows(expected.len())
            .any(|window| window == expected)
    };
    let bus = directory
        .0
        .join("appbento/dbus/appbento-0000000000000001/bus");
    assert!(has(&["--info-fd", dbus::BWRAP_INFO_PLACEHOLDER]));
    assert!(has(&[
        "--ro-bind-data",
        dbus::APP_INFO_PLACEHOLDER,
        "/.flatpak-info"
    ]));
    assert!(has(&["--bind", bus.to_str().unwrap(), "/run/user/app/bus"]));
    assert!(has(&[
        "--setenv",
        "DBUS_SESSION_BUS_ADDRESS",
        "unix:path=/run/user/app/bus"
    ]));
    assert!(has(&[
        "--bind",
        directory.0.join("doc/by-app/example").to_str().unwrap(),
        directory.0.join("doc").to_str().unwrap()
    ]));
    // The mounts must precede the program or Bubblewrap would hand them to the app.
    let separator = planned
        .arguments
        .iter()
        .position(|argument| argument == "--")
        .unwrap();
    let info = planned
        .arguments
        .iter()
        .position(|argument| argument == "--info-fd")
        .unwrap();
    assert!(info < separator);
}

#[test]
fn plan_with_desktop_integration_fails_without_a_prepared_session() {
    let (directory, mut app) = fixture();
    app.launch_config = Some(bubblewrap_config(DesktopIntegration::recommended()));
    let error = plan(&app, &[], &directory.0.join("policy"), &Runtime::default()).unwrap_err();
    assert!(error.contains("Desktop integration"), "{error}");
}

#[test]
fn invalid_custom_names_are_rejected_when_the_configuration_is_validated() {
    let (directory, mut app) = fixture();
    let policy = directory.0.join("policy");
    let config = bubblewrap_config(DesktopIntegration {
        custom: vec![crate::models::DbusRule {
            name: "org.freedesktop.DBus".into(),
            access: crate::models::DbusAccess::Talk,
        }],
        ..Default::default()
    });
    assert!(validate_config(&config, &policy).is_err());
    app.launch_config = Some(config);
    assert!(plan(&app, &[], &policy, &desktop_runtime(&directory.0, false)).is_err());
}

#[test]
fn saved_permissions_without_the_field_keep_the_desktop_off() {
    let permissions: SandboxPermissions = serde_json::from_str(r#"{"network":true}"#).unwrap();
    assert_eq!(
        permissions.desktop_integration,
        DesktopIntegration::default()
    );
    assert!(!permissions.desktop_integration.is_enabled());
    assert_eq!(
        SandboxPermissions::default().desktop_integration,
        DesktopIntegration::default()
    );

    let partial: SandboxPermissions =
        serde_json::from_str(r#"{"desktop_integration":{"secrets":true}}"#).unwrap();
    assert!(partial.desktop_integration.secrets && !partial.desktop_integration.portals);
    assert!(serde_json::from_str::<SandboxPermissions>(
        r#"{"desktop_integration":{"clipboard":true}}"#
    )
    .is_err());
}

#[test]
fn new_reviews_default_to_portals_and_notifications_only() {
    let (directory, app) = fixture();
    let permissions = default_permissions(&app, &directory.0.join("policy"));
    assert_eq!(
        permissions.desktop_integration,
        DesktopIntegration::recommended()
    );
    assert!(
        permissions.desktop_integration.portals && permissions.desktop_integration.notifications
    );
    assert!(!permissions.desktop_integration.tray);
    assert!(!permissions.desktop_integration.media_controls);
    assert!(!permissions.desktop_integration.secrets);
    assert!(permissions.desktop_integration.custom.is_empty());
}
