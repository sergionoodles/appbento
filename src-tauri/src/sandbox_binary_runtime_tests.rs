use super::*;
use crate::lifecycle::executable_tests::binary_fixture;

#[test]
fn actual_binary_loads_origin_library_resources_and_persistent_private_data() {
    let availability = status();
    if !availability.available || !Path::new("/usr/bin/cc").exists() {
        eprintln!(
            "Binary runtime check skipped: Bubblewrap {:?}; C compiler required",
            availability.error
        );
        return;
    }
    let (directory, service, binary) = binary_fixture();
    let bundle = binary.parent().unwrap();
    let library_source = directory.0.join("library.c");
    let app_source = directory.0.join("application.c");
    fs::write(&library_source, "int answer(void) { return 42; }\n").unwrap();
    fs::write(&app_source, r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
extern int answer(void);
int main(int argc, char **argv) {
    char executable[4096], resource[4096], marker[4096];
    ssize_t length = readlink("/proc/self/exe", executable, sizeof(executable) - 1);
    if (length < 0) return 1;
    executable[length] = 0;
    snprintf(resource, sizeof(resource), "%s", executable);
    char *separator = strrchr(resource, '/');
    if (!separator) return 2;
    strcpy(separator + 1, "resource.txt");
    FILE *file = fopen(resource, "r");
    if (!file) return 3;
    char text[64];
    if (!fgets(text, sizeof(text), file)) return 4;
    fclose(file);
    snprintf(marker, sizeof(marker), "%s/launch-count", getenv("HOME"));
    int count = 0;
    file = fopen(marker, "r");
    if (file) { fscanf(file, "%d", &count); fclose(file); }
    file = fopen(marker, "w");
    if (!file) return 5;
    fprintf(file, "%d", ++count);
    fclose(file);
    printf("answer=%d resource=%s argv0=%s executable=%s launches=%d\n", answer(), text, argv[0], executable, count);
    return argc == 1 ? 0 : 6;
}
"#).unwrap();
    compile(&[
        Path::new("-shared"),
        Path::new("-fPIC"),
        &library_source,
        Path::new("-o"),
        &bundle.join("libanswer.so"),
    ]);
    compile(&[
        &app_source,
        Path::new("-L"),
        bundle,
        Path::new("-lanswer"),
        Path::new("-Wl,-rpath,$ORIGIN"),
        Path::new("-o"),
        &binary,
    ]);
    fs::write(bundle.join("resource.txt"), "adjacent").unwrap();
    let app = service.import_app(&binary, false).unwrap();
    let policy = service.library_path.parent().unwrap();
    let app = service
        .configure(
            &app.id,
            &LaunchConfig {
                mode: LaunchMode::Bubblewrap,
                arguments: vec![],
                environment: Default::default(),
                permissions: SandboxPermissions {
                    display: false,
                    desktop_integration: Default::default(),
                    ..default_permissions(&app, policy)
                },
            },
            Some(&service.prepare_review(&app.id).unwrap()),
        )
        .unwrap();
    let home = private_home_path(policy, &app.id);
    fs::create_dir_all(&home).unwrap();
    for count in 1..=2 {
        let mut planned = plan(&app, &[], policy, &Runtime::default()).unwrap();
        let pinned = execution::pin_launch_paths_in_place(&mut planned, &app, policy).unwrap();
        let mut command = execution::prepare_command(&planned);
        execution::inherit_descriptors(&mut command, &planned, &pinned);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("answer=42 resource=adjacent"), "{stdout}");
        assert!(
            stdout.contains(&format!(
                "argv0={} executable={}",
                app.file_path, app.file_path
            )),
            "{stdout}"
        );
        assert!(stdout.contains(&format!("launches={count}")), "{stdout}");
        drop(pinned);
    }
}

fn compile(arguments: &[&Path]) {
    let output = Command::new("/usr/bin/cc")
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn binary_launch_logs_review_changes_and_moved_path_errors() {
    let (directory, _service, path) = binary_fixture();
    fs::copy("/usr/bin/echo", &path).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "sandbox::binary_runtime_tests::binary_review_worker",
            "--nocapture",
        ])
        .env("APPBENTO_TEST_BINARY_REVIEW", &path)
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
fn binary_review_worker() {
    let Some(path) = std::env::var_os("APPBENTO_TEST_BINARY_REVIEW") else {
        return;
    };
    let path = PathBuf::from(path);
    let service = crate::lifecycle::Lifecycle {
        library_path: crate::persistence::data_path("library.json").unwrap(),
        managed_dir: path.parent().unwrap().join("managed"),
        desktop_dir: path.parent().unwrap().join("desktop"),
    };
    let app = service.import_app(&path, false).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let error = runtime
        .block_on(crate::updates_configure(app.id.clone(), None))
        .unwrap_err();
    assert!(error.contains("Executable updates"));
    let error = runtime
        .block_on(crate::updates_install(app.id.clone()))
        .unwrap_err();
    assert!(error.contains("Executable updates"));
    let defaults = runtime
        .block_on(crate::apps::library_default_permissions(app.id.clone()))
        .unwrap();
    assert_eq!(
        defaults,
        default_permissions(&app, service.library_path.parent().unwrap())
    );
    assert!(execution::launch_observed(&app.id, &[], |_| Ok(()))
        .unwrap_err()
        .contains("Review"));
    let config = LaunchConfig {
        mode: LaunchMode::Direct,
        arguments: vec!["native launch output".into()],
        environment: Default::default(),
        permissions: SandboxPermissions::default(),
    };
    service
        .configure(
            &app.id,
            &config,
            Some(&service.prepare_review(&app.id).unwrap()),
        )
        .unwrap();
    let mut bytes = fs::read(&path).unwrap();
    bytes.push(0);
    fs::write(&path, bytes).unwrap();
    service.configure(&app.id, &config, None).unwrap();
    assert!(execution::launch_observed(&app.id, &[], |_| Ok(()))
        .unwrap_err()
        .contains("changed since review"));
    assert!(service.list().unwrap()[0].reviewed_digest.is_none());
    service
        .configure(
            &app.id,
            &config,
            Some(&service.prepare_review(&app.id).unwrap()),
        )
        .unwrap();
    let launched = execution::launch_observed(&app.id, &[], |_| Ok(())).unwrap();
    assert_eq!(launched.launch_count, 1);
    let log = serde_json::to_value(crate::launch_logs::read(&app.id).unwrap().unwrap()).unwrap();
    assert!(log["text"]
        .as_str()
        .unwrap()
        .contains("native launch output"));
    fs::rename(&path, path.with_extension("moved")).unwrap();
    let error = execution::launch_observed(&app.id, &[], |_| Ok(())).unwrap_err();
    assert!(
        error.contains("add it again from its new location"),
        "{error}"
    );
    assert!(!error.contains("scan"));
}
