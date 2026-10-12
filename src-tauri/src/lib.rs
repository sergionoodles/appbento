mod apps;
mod configuration;
mod embedded_icon;
mod github;
mod icons;
mod lifecycle;
mod models;
mod persistence;
mod sandbox;
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
    if arguments
        .first()
        .is_some_and(|argument| argument == "--launch")
    {
        if let Some(id) = arguments.get(1) {
            match store::load_all()
                .and_then(|mut library| library.remove(id).ok_or("app not found".into()))
            {
                Ok(app) if app.launch_config.is_some() && app.reviewed_digest.is_some() => {
                    if let Err(error) = sandbox::launch_with_wait(id, &arguments[2..], true) {
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

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            apps::library_list,
            apps::library_scan,
            apps::library_integrate,
            apps::library_launch,
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
    fn main_window_can_read_icons_and_manage_updates() {
        let mut context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let authority = context.runtime_authority_mut();
        for command in [
            "library_icon",
            "library_set_icon",
            "updates_configure",
            "updates_install",
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
