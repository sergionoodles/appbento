mod apps;
mod catalog;
mod github;
mod lifecycle;
mod models;
mod persistence;
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            apps::library_list,
            apps::library_scan,
            apps::library_integrate,
            apps::library_launch,
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
