//! Thin Tauri adapters for filesystem lifecycle services.
use std::path::PathBuf;

use crate::catalog;
use crate::lifecycle::{open_appimage, Lifecycle};
use crate::models::{InstalledApp, ScanReport};
use crate::{persistence, settings, store};

fn lifecycle() -> Result<Lifecycle, String> {
    let stored = settings::load()?;
    Ok(Lifecycle {
        library_path: persistence::data_path("library.json")?,
        managed_dir: PathBuf::from(stored.managed_dir),
        desktop_dir: dirs::data_dir()
            .ok_or("data directory unavailable")?
            .join("applications"),
        catalog: catalog::load_catalog()?,
    })
}

#[tauri::command]
pub async fn library_list() -> Result<Vec<InstalledApp>, String> {
    tokio::task::spawn_blocking(|| Lifecycle::list_at(&persistence::data_path("library.json")?))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_scan(extra_dirs: Option<Vec<String>>) -> Result<ScanReport, String> {
    tokio::task::spawn_blocking(move || {
        let stored = settings::load()?;
        let home = dirs::home_dir().ok_or("home directory unavailable")?;
        let defaults = vec![
            home.join("Applications"),
            home.join("Downloads"),
            home.join(".local/bin"),
            PathBuf::from("/opt/appimages"),
            PathBuf::from(&stored.managed_dir),
        ];
        let directories =
            settings::merge_scan_dirs(defaults, extra_dirs.unwrap_or_default(), stored.scan_dirs)?;
        lifecycle()?.scan(directories)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_integrate(source_path: String) -> Result<InstalledApp, String> {
    tokio::task::spawn_blocking(move || lifecycle()?.integrate(&PathBuf::from(source_path)))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_launch(id: String) -> Result<InstalledApp, String> {
    let launch_id = id.clone();
    let app = tokio::task::spawn_blocking(move || {
        let app = store::load_all()?
            .remove(&launch_id)
            .ok_or("app not found")?;
        let _file = open_appimage(&PathBuf::from(&app.file_path))?;
        Ok::<_, String>(app)
    })
    .await
    .map_err(|error| error.to_string())??;
    let mut child = tokio::process::Command::new(&app.file_path)
        .spawn()
        .map_err(|error| format!("launch failed: {error}"))?;
    tokio::spawn(async move {
        if let Err(error) = child.wait().await {
            eprintln!("could not wait for launched AppImage: {error}");
        }
    });
    tokio::task::spawn_blocking(move || store::touch_launch(&id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_remove(id: String, delete_file: bool) -> Result<(), String> {
    tokio::task::spawn_blocking(move || lifecycle()?.remove(&id, delete_file))
        .await
        .map_err(|error| error.to_string())?
}
