mod app_identity;
mod apps;
mod configuration;
mod embedded_icon;
mod file_association;
mod github;
mod icons;
mod import_download;
mod launch_logs;
mod launch_monitor;
mod lifecycle;
mod models;
mod persistence;
mod sandbox;
mod self_launcher;
mod self_update;
mod self_update_installation;
mod settings;
mod store;
mod update_download;
mod update_sources;
mod updates;

use models::{UpdateCheckResult, UpdateReport};

async fn update_snapshot() -> Result<Vec<models::InstalledApp>, String> {
    tokio::task::spawn_blocking(|| Ok(store::load_all()?.into_values().collect()))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn updates_check(app_id: String) -> Result<UpdateCheckResult, String> {
    let apps = update_snapshot().await?;
    let app = apps
        .iter()
        .find(|app| app.id == app_id)
        .ok_or("app not found")?;
    Ok(updates::check_app(app).await)
}

#[tauri::command]
async fn updates_check_all() -> Result<UpdateReport, String> {
    Ok(updates::check_all(update_snapshot().await?).await)
}

#[tauri::command]
async fn updates_configure(
    app_id: String,
    config: Option<models::UpdateConfig>,
) -> Result<models::InstalledApp, String> {
    if let Some(config) = &config {
        update_sources::validate_config(config)?;
    }
    tokio::task::spawn_blocking(move || {
        store::mutate_library(|library| {
            let app = library.get_mut(&app_id).ok_or("app not found")?;
            app.update_config = config;
            Ok(app.clone())
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn updates_install(app_id: String) -> Result<models::InstalledApp, String> {
    let app = update_snapshot()
        .await?
        .into_iter()
        .find(|app| app.id == app_id)
        .ok_or("app not found")?;
    let lifecycle = tokio::task::spawn_blocking(|| apps::lifecycle())
        .await
        .map_err(|error| error.to_string())??;
    update_download::install(app, lifecycle).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some(launch_monitor::ARGUMENT) {
        let result = arguments
            .get(1)
            .ok_or_else(|| "app id missing".to_owned())
            .and_then(|id| launch_monitor::run(id, &arguments[2..]));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "--launch")
    {
        if let Some(id) = arguments.get(1) {
            match store::load_all()
                .and_then(|mut library| library.remove(id).ok_or("app not found".into()))
            {
                Ok(app) if app.launch_config.is_some() && app.reviewed_digest.is_some() => {
                    if let Err(error) = sandbox::launch(id, &arguments[2..]) {
                        eprintln!("{error}");
                        std::process::exit(1);
                    }
                    return;
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            }
        }
    }

    let open_request =
        file_association::OpenRequest::new(file_association::requested_path(&arguments));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(self_update::SelfUpdateState::default())
        .manage(open_request)
        .setup(|_| {
            self_update_installation::notify_startup()?;
            file_association::register_at_startup();
            Ok(())
        })
        .on_window_event(|window, event| {
            use std::sync::atomic::Ordering;
            use tauri::Manager;
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window
                    .state::<self_update::SelfUpdateState>()
                    .is_installing
                    .load(Ordering::Acquire)
                {
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            self_launcher::self_launcher_status,
            self_launcher::self_launcher_install,
            self_launcher::self_launcher_dismiss,
            self_update::self_update_check,
            self_update::self_update_install,
            file_association::open_request_take,
            apps::library_list,
            apps::library_scan,
            apps::library_import,
            apps::library_set_menu_entry,
            apps::library_add_url,
            apps::library_prepare_launch,
            apps::library_launch,
            apps::library_launch_log,
            apps::library_configure,
            apps::library_set_icon,
            apps::library_icon,
            apps::sandbox_status,
            apps::library_remove,
            updates_check,
            updates_check_all,
            updates_configure,
            updates_install,
            configuration::configuration_export,
            configuration::configuration_read,
            configuration::configuration_restore,
            configuration::configuration_preferences,
            settings::settings_get,
            settings::settings_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AppBento");
}

#[cfg(test)]
mod permission_tests {
    #[test]
    fn main_window_can_read_latest_launch_logs() {
        let mut context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        assert!(
            context
                .runtime_authority_mut()
                .resolve_access(
                    "library_launch_log",
                    "main",
                    "main",
                    &tauri::ipc::Origin::Local
                )
                .is_some(),
            "the main window must be allowed to read latest launch logs"
        );
    }

    #[test]
    fn main_window_can_read_icons_and_manage_updates() {
        let mut context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let authority = context.runtime_authority_mut();
        for command in [
            "library_icon",
            "library_add_url",
            "library_set_icon",
            "updates_configure",
            "updates_install",
            "self_launcher_status",
            "self_launcher_install",
            "self_launcher_dismiss",
            "self_update_check",
            "self_update_install",
            "open_request_take",
        ] {
            assert!(
                authority
                    .resolve_access(command, "main", "main", &tauri::ipc::Origin::Local)
                    .is_some(),
                "the main window must be allowed to invoke {command}"
            );
        }
    }
}
