use super::*;
use crate::models::PathPermission;
use crate::persistence::tests::TemporaryDirectory;
use std::time::{Duration, Instant};

const TEST_DIRECTORY: &str = "APPBENTO_TEST_SANDBOX_LIFETIME_DIRECTORY";

#[test]
fn sandbox_outlives_request_thread() {
    if !status().available || !Path::new("/usr/bin/cc").exists() {
        eprintln!("Sandbox lifetime test requires Bubblewrap and a C compiler");
        return;
    }
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("fixture.c");
    let images = directory.0.join("images");
    fs::create_dir(&images).unwrap();
    let image = images.join("Example.AppImage");
    fs::write(
        &source,
        "#include <stdio.h>\n#include <unistd.h>\nint main(int argc, char **argv) {\n  if (argc != 3) return 1;\n  if (argv[2][0] == '!') return 7;\n  usleep(500000);\n  FILE *file = fopen(argv[2], \"w\");\n  if (!file) return 2;\n  fputs(\"completed\", file);\n  return fclose(file);\n}\n",
    )
    .unwrap();
    assert!(Command::new("/usr/bin/cc")
        .arg(&source)
        .arg("-o")
        .arg(&image)
        .status()
        .unwrap()
        .success());
    let mut bytes = fs::read(&image).unwrap();
    bytes[8..11].copy_from_slice(b"AI\x02");
    fs::write(&image, bytes).unwrap();

    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "sandbox::lifetime_tests::sandbox_lifetime_worker",
            "--nocapture",
        ])
        .env(TEST_DIRECTORY, &directory.0)
        .env("XDG_DATA_HOME", directory.0.join("data"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sandbox_lifetime_worker() {
    let Some(directory) = std::env::var_os(TEST_DIRECTORY) else {
        return;
    };
    let directory = PathBuf::from(directory);
    let output = directory.join("output");
    fs::create_dir(&output).unwrap();
    let marker = output.join("completed");
    let lifecycle = crate::lifecycle::Lifecycle {
        library_path: crate::persistence::data_path("library.json").unwrap(),
        managed_dir: directory.join("images"),
        desktop_dir: directory.join("desktop"),
    };
    let app = lifecycle
        .scan(vec![directory.join("images")])
        .unwrap()
        .apps
        .remove(0);
    lifecycle
        .configure(
            &app.id,
            &LaunchConfig {
                arguments: vec![marker.to_str().unwrap().into()],
                environment: Default::default(),
                mode: LaunchMode::Bubblewrap,
                permissions: SandboxPermissions {
                    persistent_data: false,
                    paths: vec![PathPermission {
                        path: output.to_str().unwrap().into(),
                        access: PathAccess::ReadWrite,
                    }],
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let launched = std::thread::spawn(move || launch(&app.id, &[]))
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(launched.launch_count, 1);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fs::read_to_string(marker).unwrap(), "completed");

    let failed_config = LaunchConfig {
        arguments: vec!["!fail".into()],
        ..launched.launch_config.unwrap()
    };
    lifecycle.configure(&launched.id, &failed_config).unwrap();
    let id = launched.id;
    let error = std::thread::spawn(move || launch(&id, &[]))
        .join()
        .unwrap()
        .unwrap_err();
    assert!(error.contains("AppImage launch failed"), "{error}");
}
