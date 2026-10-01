mod apps;
mod catalog;
mod embedded_icon;
mod github;
mod icons;
mod lifecycle;
mod models;
mod persistence;
mod sandbox;
mod settings;
mod store;
mod updates;

use models::{CatalogEntry, UpdateCheckResult, UpdateReport};

#[tauri::command]
fn catalog_list() -> Result<Vec<CatalogEntry>, String> {
    catalog::load_catalog()
}

#[tauri::command]
fn catalog_get(id: String) -> Result<Option<CatalogEntry>, String> {
    Ok(catalog::load_catalog()?
        .into_iter()
        .find(|entry| entry.app.id == id))
}

async fn update_snapshot() -> Result<(Vec<models::InstalledApp>, Vec<CatalogEntry>), String> {
    tokio::task::spawn_blocking(|| {
        Ok((
            store::load_all()?.into_values().collect(),
            catalog::load_catalog()?,
        ))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn updates_check(app_id: String) -> Result<UpdateCheckResult, String> {
    let (apps, entries) = update_snapshot().await?;
    let app = apps
        .iter()
        .find(|app| app.id == app_id)
        .ok_or("app not found")?;
    Ok(updates::check_app(app, &entries).await)
}

#[tauri::command]
async fn updates_check_all() -> Result<UpdateReport, String> {
    let (apps, entries) = update_snapshot().await?;
    Ok(updates::check_all(apps, entries).await)
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
                Ok(app) if app.launch_config.is_some() => {
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
            catalog_list,
            catalog_get,
            updates_check,
            updates_check_all,
            settings::settings_get,
            settings::settings_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AppBento");
}

#[cfg(test)]
mod permission_tests {
    #[test]
    fn main_window_can_read_and_customize_app_icons() {
        let mut context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let authority = context.runtime_authority_mut();
        for command in ["library_icon", "library_set_icon"] {
            assert!(
                authority
                    .resolve_access(command, "main", "main", &tauri::ipc::Origin::Local)
                    .is_some(),
                "the main window must be allowed to invoke {command}"
            );
        }
    }
}
