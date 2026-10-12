use super::*;
use crate::persistence::tests::TemporaryDirectory;

#[test]
fn sandbox_can_read_system_ca_bundles_without_network_access() {
    if !status().available {
        eprintln!("CA bundle test requires Bubblewrap");
        return;
    }
    let bundles = [
        "/etc/ssl/cert.pem",
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/tls/cert.pem",
        "/etc/pki/tls/certs/ca-bundle.crt",
    ]
    .into_iter()
    .filter(|path| Path::new(path).is_file())
    .collect::<Vec<_>>();
    if bundles.is_empty() {
        eprintln!("CA bundle test requires a standard system certificate store");
        return;
    }
    let directory = TemporaryDirectory::new();
    let image = directory.0.join("Example.AppImage");
    let mut bytes = fs::read("/usr/bin/true").unwrap();
    bytes[8..11].copy_from_slice(b"AI\x02");
    fs::write(&image, bytes).unwrap();
    let lifecycle = crate::lifecycle::Lifecycle {
        library_path: directory.0.join("policy/library.json"),
        managed_dir: directory.0.join("managed"),
        desktop_dir: directory.0.join("desktop"),
    };
    let mut app = lifecycle
        .scan(vec![directory.0.clone()])
        .unwrap()
        .apps
        .remove(0);
    app.launch_config = Some(LaunchConfig {
        arguments: Vec::new(),
        environment: Default::default(),
        mode: LaunchMode::Bubblewrap,
        permissions: SandboxPermissions {
            persistent_data: false,
            network: false,
            ..Default::default()
        },
    });
    let mut planned = plan(&app, &[], &directory.0.join("policy"), &Runtime::default()).unwrap();
    assert!(!planned
        .arguments
        .iter()
        .any(|argument| argument == "--share-net"));
    let command_index = planned
        .arguments
        .iter()
        .position(|argument| argument == "--")
        .unwrap();
    planned.arguments.truncate(command_index + 1);
    planned.arguments.extend([
        "/bin/sh".into(),
        "-c".into(),
        "for bundle in \"$@\"; do test -s \"$bundle\" && test -r \"$bundle\" || exit 1; done; test ! -e /etc/ssl/private && test ! -e /etc/resolv.conf".into(),
        "certificate-check".into(),
    ]);
    planned
        .arguments
        .extend(bundles.into_iter().map(str::to_owned));
    let output = Command::new(planned.program)
        .args(planned.arguments)
        .env_clear()
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
