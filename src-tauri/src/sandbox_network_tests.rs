use super::*;

#[test]
fn restricted_launch_rejects_network_without_display_permission() {
    let (directory, mut app) = tests::fixture();
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Bubblewrap,
        arguments: Vec::new(),
        environment: Default::default(),
        permissions: SandboxPermissions {
            network: true,
            display: false,
            ..Default::default()
        },
    });
    let result = plan(&app, &[], &directory.0.join("policy"), &Runtime::default());
    assert!(
        result.is_err(),
        "host networking must require explicit Display permission"
    );
    assert!(result.unwrap_err().contains("Display"));
}

#[test]
fn network_display_policy_is_explicit_even_before_an_x11_server_starts() {
    let (directory, mut app) = tests::fixture();
    for (network, display, is_allowed) in [
        (true, false, false),
        (true, true, true),
        (false, false, true),
        (false, true, true),
    ] {
        app.launch_config = Some(LaunchConfig {
            mode: LaunchMode::Bubblewrap,
            arguments: Vec::new(),
            environment: Default::default(),
            permissions: SandboxPermissions {
                network,
                display,
                ..Default::default()
            },
        });
        let result = plan(&app, &[], &directory.0.join("policy"), &Runtime::default());
        assert_eq!(
            result.is_ok(),
            is_allowed,
            "network={network} display={display}"
        );
    }
}

#[test]
fn direct_launch_keeps_its_existing_unrestricted_behavior() {
    let (directory, mut app) = tests::fixture();
    app.launch_config = Some(LaunchConfig {
        mode: LaunchMode::Direct,
        arguments: Vec::new(),
        environment: Default::default(),
        permissions: SandboxPermissions {
            network: true,
            display: false,
            ..Default::default()
        },
    });
    let planned = plan(&app, &[], &directory.0.join("policy"), &Runtime::default()).unwrap();
    assert!(!planned.clear_environment);
}
