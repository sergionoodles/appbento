use super::*;
use crate::models::PathPermission;
use crate::persistence::tests::TemporaryDirectory;
use std::io::Write;
use std::time::{Duration, Instant};

const TEST_DIRECTORY: &str = "APPBENTO_TEST_DETACHMENT_DIRECTORY";
const TEST_MODE: &str = "APPBENTO_TEST_DETACHMENT_MODE";
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

unsafe extern "C" {
    fn getsid(pid: i32) -> i32;
}

#[test]
fn direct_app_outlives_launcher_process_and_can_write_output() {
    assert_app_outlives_launcher("direct", false);
}

#[test]
fn packaged_launcher_starts_an_independent_appimage_runtime() {
    assert_app_outlives_launcher("direct", true);
}

#[test]
fn sandboxed_app_outlives_launcher_process_and_can_write_output() {
    if !status().available {
        eprintln!("Detachment test requires Bubblewrap");
        return;
    }
    assert_app_outlives_launcher("bubblewrap", false);
}

fn assert_app_outlives_launcher(mode: &str, is_packaged: bool) {
    if !Path::new("/usr/bin/cc").exists() {
        eprintln!("Detachment test requires a C compiler");
        return;
    }
    let directory = TemporaryDirectory::new();
    let images = directory.0.join("images");
    fs::create_dir(&images).unwrap();
    create_appimage_fixture(&images);

    let executable = std::env::current_exe().unwrap();
    let mut command = Command::new(&executable);
    command
        .args([
            "--exact",
            "sandbox::detachment_tests::launcher_worker",
            "--nocapture",
        ])
        .env(TEST_DIRECTORY, &directory.0)
        .env(TEST_MODE, mode)
        .env("XDG_DATA_HOME", directory.0.join("data"));
    if is_packaged {
        use std::os::unix::fs::PermissionsExt;

        let runtime = directory.0.join("AppBento runtime.AppImage");
        fs::write(
            &runtime,
            "#!/bin/sh\nprintf started > \"$APPBENTO_TEST_DETACHMENT_DIRECTORY/runtime-started\"\nexec \"$APPBENTO_TEST_EXECUTABLE\" \"$@\"\n",
        )
        .unwrap();
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
        command
            .env("APPIMAGE", runtime)
            .env("APPBENTO_TEST_EXECUTABLE", executable);
    }
    let mut worker = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    worker
        .stdin
        .take()
        .unwrap()
        .write_all(b"direct-input\n")
        .unwrap();
    let output = worker.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if is_packaged {
        assert!(directory.0.join("runtime-started").is_file());
    }
    // The launcher has exited before the app writes to either captured stream.
    let app_output = directory.0.join("output");
    fs::write(app_output.join("release"), "").unwrap();
    let marker = app_output.join("completed");
    let deadline = Instant::now() + COMPLETION_TIMEOUT;
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fs::read_to_string(marker).expect("app must survive launcher exit and output writes"),
        "completed"
    );
    assert_logs_and_cleanup(&directory.0);
}

fn create_appimage_fixture(images: &Path) {
    let source = images.join("fixture.c");
    fs::write(
        &source,
        r#"#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc < 4) return 1;
    if (getsid(0) == atoi(argv[argc - 2])) return 7;
    if (chdir(argv[argc - 1]) != 0) return 2;
    for (int attempt = 0; access("release", F_OK) != 0; attempt++) {
        if (attempt == 500) return 3;
        usleep(10000);
    }
    if (strcmp(argv[argc - 3], "read-stdin") == 0) {
        char input[32];
        if (!fgets(input, sizeof input, stdin) || strcmp(input, "direct-input\n") != 0) return 8;
    }
    if (printf("stdout after launcher exit\n") < 0 || fflush(stdout) != 0) return 4;
    if (fprintf(stderr, "stderr after launcher exit\n") < 0 || fflush(stderr) != 0) return 5;
    FILE *file = fopen("completed", "w");
    if (!file) return 6;
    fputs("completed", file);
    return fclose(file);
}
"#,
    )
    .unwrap();
    let image = images.join("Example.AppImage");
    assert!(Command::new("/usr/bin/cc")
        .arg(source)
        .arg("-o")
        .arg(&image)
        .status()
        .unwrap()
        .success());
    let mut bytes = fs::read(&image).unwrap();
    bytes[8..11].copy_from_slice(b"AI\x02");
    fs::write(image, bytes).unwrap();
}

fn assert_logs_and_cleanup(directory: &Path) {
    let log_path = fs::read_to_string(directory.join("log-path")).unwrap();
    let policy_directory = directory.join("data/appbento");
    let deadline = Instant::now() + COMPLETION_TIMEOUT;
    loop {
        let log: serde_json::Value = serde_json::from_slice(&fs::read(&log_path).unwrap()).unwrap();
        let lines = log["lines"].as_array().unwrap();
        let has_temporary_files = fs::read_dir(&policy_directory)
            .unwrap()
            .any(|entry| entry.unwrap().path().extension() == Some(std::ffi::OsStr::new("tmp")));
        if lines.contains(&"stdout after launcher exit".into())
            && lines.contains(&"stderr after launcher exit".into())
            && !has_temporary_files
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "output must remain captured and launch snapshots must be cleaned after exit"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn launcher_worker() {
    let Some(directory) = std::env::var_os(TEST_DIRECTORY) else {
        return;
    };
    let directory = PathBuf::from(directory);
    let output = directory.join("output");
    fs::create_dir(&output).unwrap();
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
    let mode = if std::env::var(TEST_MODE).unwrap() == "direct" {
        LaunchMode::Direct
    } else {
        LaunchMode::Bubblewrap
    };
    lifecycle
        .configure(
            &app.id,
            &LaunchConfig {
                arguments: vec![
                    if mode == LaunchMode::Direct {
                        "read-stdin"
                    } else {
                        "no-stdin"
                    }
                    .into(),
                    unsafe { getsid(0) }.to_string(),
                    output.to_str().unwrap().into(),
                ],
                environment: Default::default(),
                mode,
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
    assert_eq!(launch(&app.id, &[]).unwrap().launch_count, 1);
    let log_path = crate::persistence::data_path(&format!(
        "launch-logs/{:x}.json",
        Sha256::digest(app.id.as_bytes())
    ))
    .unwrap();
    fs::write(directory.join("log-path"), log_path.to_str().unwrap()).unwrap();
}
