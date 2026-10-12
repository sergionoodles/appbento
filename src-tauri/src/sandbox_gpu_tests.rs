use super::*;
use crate::persistence::tests::TemporaryDirectory;

#[test]
fn gpu_permission_exposes_device_metadata_read_only() {
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
    for gpu in [false, true] {
        app.launch_config = Some(LaunchConfig {
            arguments: Vec::new(),
            environment: Default::default(),
            mode: LaunchMode::Bubblewrap,
            permissions: SandboxPermissions {
                gpu,
                ..Default::default()
            },
        });
        let planned = plan(&app, &[], &directory.0.join("policy"), &Runtime::default()).unwrap();
        for path in ["/sys/dev/char", "/sys/devices"] {
            let is_mounted = planned
                .arguments
                .windows(3)
                .any(|arguments| arguments == ["--ro-bind", path, path]);
            assert_eq!(
                is_mounted,
                gpu && Path::new("/dev/dri").exists() && Path::new(path).exists()
            );
            assert!(!planned.arguments.windows(3).any(|arguments| {
                arguments == ["--bind", path, path] || arguments == ["--dev-bind", path, path]
            }));
        }
    }
}
