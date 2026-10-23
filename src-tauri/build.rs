const COMMANDS: &[&str] = &[
    "self_launcher_status",
    "self_launcher_install",
    "self_launcher_dismiss",
    "self_update_check",
    "self_update_install",
    "open_request_take",
    "library_list",
    "library_icon",
    "library_set_icon",
    "library_scan",
    "library_import",
    "library_set_menu_entry",
    "library_add_url",
    "library_prepare_launch",
    "library_launch",
    "library_launch_log",
    "library_configure",
    "sandbox_status",
    "library_remove",
    "updates_check",
    "updates_check_all",
    "updates_configure",
    "updates_install",
    "configuration_export",
    "configuration_read",
    "configuration_restore",
    "configuration_preferences",
    "settings_get",
    "settings_set",
];

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
