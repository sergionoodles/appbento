//! Thin Tauri adapters for filesystem lifecycle services.
use std::path::PathBuf;

use crate::lifecycle::Lifecycle;
use crate::models::{InstalledApp, LaunchConfig, ScanReport};
use crate::{icons, persistence, settings};

pub(crate) fn lifecycle() -> Result<Lifecycle, String> {
    let stored = settings::load()?;
    Ok(Lifecycle {
        library_path: persistence::data_path("library.json")?,
        managed_dir: PathBuf::from(stored.managed_dir),
        desktop_dir: dirs::data_dir()
            .ok_or("data directory unavailable")?
            .join("applications"),
    })
}

#[tauri::command]
pub async fn library_list() -> Result<Vec<InstalledApp>, String> {
    tokio::task::spawn_blocking(|| {
        let mut apps = lifecycle()?.list()?;
        let arguments = std::env::args().skip(1).collect::<Vec<_>>();
        if arguments
            .first()
            .is_some_and(|argument| argument == "--launch")
        {
            if let Some(id) = arguments.get(1) {
                apps.sort_by_key(|app| app.id != *id);
            }
        }
        Ok::<_, String>(apps)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_scan(extra_dirs: Option<Vec<String>>) -> Result<ScanReport, String> {
    let mut report = tokio::task::spawn_blocking(move || {
        let stored = settings::load()?;
        let directories = settings::merge_scan_dirs(
            extra_dirs.unwrap_or_default(),
            stored
                .scan_dirs
                .into_iter()
                .chain(std::iter::once(stored.managed_dir))
                .collect(),
        )?;
        lifecycle()?.scan(directories)
    })
    .await
    .map_err(|error| error.to_string())??;
    for app in &mut report.apps {
        match discover_icon(app).await {
            Ok(discovered) => *app = discovered,
            Err(message) => report.warnings.push(crate::models::ScanWarning {
                path: app.file_path.clone(),
                message: format!("icon discovery failed: {message}"),
            }),
        }
    }
    Ok(report)
}

#[tauri::command]
pub async fn library_add_url(url: String) -> Result<InstalledApp, String> {
    let service = tokio::task::spawn_blocking(lifecycle)
        .await
        .map_err(|error| error.to_string())??;
    let app = crate::import_download::install(&url, service).await?;
    match discover_icon(&app).await {
        Ok(discovered) => Ok(discovered),
        Err(error) => {
            eprintln!("icon discovery failed for {}: {error}", app.id);
            Ok(app)
        }
    }
}

#[tauri::command]
pub async fn library_import(source_path: String) -> Result<InstalledApp, String> {
    let app =
        tokio::task::spawn_blocking(move || lifecycle()?.integrate(&PathBuf::from(source_path)))
            .await
            .map_err(|error| error.to_string())??;
    match discover_icon(&app).await {
        Ok(discovered) => Ok(discovered),
        Err(error) => {
            eprintln!("icon discovery failed for {}: {error}", app.name);
            Ok(app)
        }
    }
}

#[tauri::command]
pub async fn library_set_menu_entry(id: String, enabled: bool) -> Result<InstalledApp, String> {
    tokio::task::spawn_blocking(move || lifecycle()?.set_menu_entry(&id, enabled))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_prepare_launch(
    id: String,
    make_executable: bool,
) -> Result<InstalledApp, String> {
    tokio::task::spawn_blocking(move || lifecycle()?.prepare_launch(&id, make_executable))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_launch(id: String) -> Result<InstalledApp, String> {
    tokio::task::spawn_blocking(move || crate::sandbox::launch(&id, &[]))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_launch_log(
    id: String,
) -> Result<Option<crate::launch_logs::LaunchLog>, String> {
    tokio::task::spawn_blocking(move || crate::launch_logs::read(&id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_remove(id: String, delete_file: bool) -> Result<(), String> {
    tokio::task::spawn_blocking(move || lifecycle()?.remove(&id, delete_file))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn sandbox_status() -> Result<crate::sandbox::SandboxStatus, String> {
    tokio::task::spawn_blocking(crate::sandbox::status)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn library_configure(id: String, config: LaunchConfig) -> Result<InstalledApp, String> {
    tokio::task::spawn_blocking(move || lifecycle()?.configure(&id, &config))
        .await
        .map_err(|error| error.to_string())?
}

async fn discover_icon(app: &InstalledApp) -> Result<InstalledApp, String> {
    if app.icon_path.is_some() {
        return Ok(app.clone());
    }
    let path = PathBuf::from(&app.file_path);
    let embedded = tokio::task::spawn_blocking(move || crate::embedded_icon::discover(&path))
        .await
        .map_err(|error| error.to_string())?;
    let (embedded_icon, website, extraction_error) = match embedded {
        Ok(metadata) => (metadata.icon, metadata.website, None),
        Err(error) => (None, None, Some(error)),
    };
    let bytes = match embedded_icon.filter(|bytes| icons::image_format(bytes).is_ok()) {
        Some(bytes) => bytes,
        None => {
            if website.is_none() {
                return match extraction_error {
                    Some(error) => Err(error),
                    None => Ok(app.clone()),
                };
            }
            icons::discover(
                &icons::client()?,
                website.as_deref().ok_or("website unavailable")?,
            )
            .await?
        }
    };
    let id = app.id.clone();
    tokio::task::spawn_blocking(move || persist_icon(&id, &bytes, false))
        .await
        .map_err(|error| error.to_string())?
}

fn persist_icon(id: &str, bytes: &[u8], replace: bool) -> Result<InstalledApp, String> {
    icons::persist_icon(
        &persistence::data_path("library.json")?,
        &persistence::data_path("icons")?,
        id,
        bytes,
        replace,
    )
}

#[tauri::command]
pub async fn library_set_icon(id: String, source: String) -> Result<InstalledApp, String> {
    let exists = id.clone();
    tokio::task::spawn_blocking(move || {
        crate::store::load_all()?
            .get(&exists)
            .ok_or_else(|| "app not found".to_string())
            .map(|_| ())
    })
    .await
    .map_err(|error| error.to_string())??;
    let bytes = icons::acquire(&icons::client()?, &source).await?;
    tokio::task::spawn_blocking(move || persist_icon(&id, &bytes, true))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn library_icon(id: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        let library = crate::store::load_all()?;
        let app = library.get(&id).ok_or("app not found")?;
        app.icon_path
            .as_deref()
            .map(|path| icons::data_url(&persistence::data_path("icons")?, path))
            .transpose()
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
#[path = "apps_tests.rs"]
mod tests;
