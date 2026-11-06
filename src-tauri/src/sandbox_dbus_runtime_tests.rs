use super::*;
use crate::models::DesktopIntegration;
use crate::persistence::tests::TemporaryDirectory;
use std::time::{Duration, Instant};

const TEST_DIRECTORY: &str = "APPBENTO_TEST_SANDBOX_DBUS_DIRECTORY";
const WAIT: Duration = Duration::from_secs(10);
const PORTAL_VERSION_CALL: [&str; 9] = [
    "call",
    "--session",
    "-d",
    "org.freedesktop.portal.Desktop",
    "-o",
    "/org/freedesktop/portal/desktop",
    "-m",
    "org.freedesktop.DBus.Properties.Get",
    "org.freedesktop.portal.FileChooser",
];
const FIXTURE: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <dirent.h>
static int descriptors_are_private(char *leak, size_t capacity) {
  DIR *directory = opendir("/proc/self/fd");
  if (!directory) return 0;
  int is_private = 1;
  struct dirent *entry;
  while ((entry = readdir(directory))) {
    if (entry->d_name[0] == '.') continue;
    int descriptor = atoi(entry->d_name);
    if (descriptor > STDERR_FILENO && descriptor != dirfd(directory)) {
      char path[128], target[4096];
      snprintf(path, sizeof(path), "/proc/self/fd/%d", descriptor);
      ssize_t length = readlink(path, target, sizeof(target) - 1);
      if (length < 0) continue;
      target[length] = '\0';
      snprintf(leak, capacity, "descriptor %d: %s", descriptor, target);
      is_private = 0;
      break;
    }
  }
  closedir(directory);
  return is_private;
}
static int touch(const char *directory, const char *name, const char *text) {
  char path[4096];
  snprintf(path, sizeof(path), "%s/%s", directory, name);
  FILE *file = fopen(path, "w");
  if (!file) return -1;
  fputs(text, file);
  return fclose(file);
}
int main(int argc, char **argv) {
  if (argc != 3) return 1;
  char path[4096], command[8192], leak[4096] = "cannot inspect descriptors";
  if (touch(argv[2], "descriptors", descriptors_are_private(leak, sizeof(leak)) ? "private" : leak)) return 4;
  if (touch(argv[2], "started", "started")) return 2;
  snprintf(path, sizeof(path), "%s/release", argv[2]);
  for (int i = 0; i < 200 && access(path, F_OK) != 0; i++) usleep(50000);
  snprintf(command, sizeof(command),
    "{ cat /.flatpak-info; "
    "gdbus call --session -d org.freedesktop.portal.Desktop -o /org/freedesktop/portal/desktop "
    "-m org.freedesktop.DBus.Properties.Get org.freedesktop.portal.FileChooser version; "
    "gdbus call --session -d org.freedesktop.secrets -o /org/freedesktop/secrets "
    "-m org.freedesktop.DBus.Peer.Ping; } > '%s/result' 2>&1", argv[2]);
  system(command);
  return touch(argv[2], "done", "done") ? 3 : 0;
}
"#;

fn host_can_run_the_scenario() -> Result<(), String> {
    let availability = status();
    if !availability.available || !availability.desktop_integration_available {
        return Err(format!(
            "Bubblewrap {:?}; desktop integration {:?}",
            availability.error, availability.desktop_integration_error
        ));
    }
    if !Path::new("/usr/bin/cc").exists() || !Path::new("/usr/bin/gdbus").exists() {
        return Err("a C compiler and gdbus are required".into());
    }
    let portal = Command::new("/usr/bin/gdbus")
        .args(PORTAL_VERSION_CALL)
        .arg("version")
        .output()
        .map_err(|error| error.to_string())?;
    if !portal.status.success() {
        return Err("the host has no xdg-desktop-portal on its session bus".into());
    }
    Ok(())
}

#[test]
fn proxied_bus_reaches_only_allowed_services_and_cleans_up_after_exit() {
    if let Err(reason) = host_can_run_the_scenario() {
        eprintln!("Desktop integration runtime check skipped: {reason}");
        return;
    }
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("fixture.c");
    let images = directory.0.join("images");
    fs::create_dir(&images).unwrap();
    let image = images.join("Example.AppImage");
    fs::write(&source, FIXTURE).unwrap();
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
            "sandbox::dbus_runtime_tests::desktop_integration_worker",
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

fn wait_for(description: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn instance_directories(runtime: &Path, app_id: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(runtime.join(".flatpak")) else {
        return Vec::new();
    };
    entries
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            fs::read_to_string(path.join("info"))
                .is_ok_and(|info| info.contains(&format!("name={app_id}\n")))
        })
        .collect()
}

fn holds_read_lock(file: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let inode = fs::metadata(file).unwrap().ino();
    fs::read_to_string("/proc/locks")
        .unwrap()
        .lines()
        .any(|line| line.contains(" READ ") && line.contains(&format!(":{inode} ")))
}

#[test]
fn desktop_integration_worker() {
    let Some(directory) = std::env::var_os(TEST_DIRECTORY) else {
        return;
    };
    let directory = PathBuf::from(directory);
    let output = directory.join("output");
    fs::create_dir(&output).unwrap();
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
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
    let app_id = dbus::app_id(&app.id);
    lifecycle
        .configure(
            &app.id,
            &LaunchConfig {
                arguments: vec![output.to_str().unwrap().into()],
                environment: Default::default(),
                mode: LaunchMode::Bubblewrap,
                permissions: SandboxPermissions {
                    persistent_data: true,
                    desktop_integration: DesktopIntegration {
                        portals: true,
                        ..Default::default()
                    },
                    paths: vec![PathPermission {
                        path: output.to_str().unwrap().into(),
                        access: PathAccess::ReadWrite,
                    }],
                    ..Default::default()
                },
            },
            Some(&lifecycle.prepare_review(&app.id).unwrap()),
        )
        .unwrap();
    let id = app.id.clone();
    std::thread::spawn(move || launch(&id, &[]))
        .join()
        .unwrap()
        .unwrap();

    wait_for("the app to start", || output.join("started").exists());
    let [instance] = instance_directories(&runtime, &app_id).try_into().unwrap();
    let proxy_directory = runtime
        .join("appbento/dbus")
        .join(instance.file_name().unwrap());
    wait_for("bwrapinfo.json", || {
        fs::metadata(instance.join("bwrapinfo.json")).is_ok_and(|metadata| metadata.len() > 0)
    });
    assert!(proxy_directory.join("bus").exists());
    assert!(holds_read_lock(&instance.join(".ref")));

    fs::write(output.join("release"), "").unwrap();
    wait_for("the app to finish", || output.join("done").exists());
    assert_eq!(
        fs::read_to_string(output.join("descriptors")).unwrap(),
        "private"
    );
    let result = fs::read_to_string(output.join("result")).unwrap();
    assert!(result.contains(&format!("name={app_id}")), "{result}");
    assert!(result.contains("(<uint32 "), "portal unreachable: {result}");
    assert!(
        result.contains("ServiceUnknown"),
        "secrets reachable: {result}"
    );

    wait_for("directories to be removed", || {
        !instance.exists() && !proxy_directory.exists()
    });
}
