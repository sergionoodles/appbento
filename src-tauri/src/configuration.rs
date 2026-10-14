//! Portable settings backups; AppImage binaries and application data stay separate.
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::models::InstalledApp;
use crate::persistence::{self, FileChange, Repository};
use crate::settings::Settings;
use crate::store::Library;

const FORMAT: &str = "appbento-configuration";
const FORMAT_VERSION: u32 = 1;
const BACKUP_LIMIT: u64 = 128 * 1024 * 1024;
const ICON_LIMIT: usize = 5 * 1024 * 1024;
const NONBLOCKING: i32 = 0x800;
const NO_FOLLOW: i32 = 0x20000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(rename = "appIds")]
    pub app_ids: Vec<String>,
    #[serde(rename = "isCollapsed")]
    pub is_collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub ungrouped: Vec<String>,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub theme: String,
    pub layout: Layout,
    pub display_names: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppBackup {
    pub app: InstalledApp,
    pub icon: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationBackup {
    pub format: String,
    pub version: u32,
    pub home_directory: String,
    pub settings: Settings,
    pub preferences: Preferences,
    pub apps: Vec<AppBackup>,
}

#[derive(Serialize)]
pub struct ImportPreview {
    pub backup: ConfigurationBackup,
    pub settings: Settings,
    pub app_paths: BTreeMap<String, String>,
    pub permission_paths: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreRequest {
    pub backup: ConfigurationBackup,
    pub settings: Settings,
    pub app_paths: BTreeMap<String, String>,
    pub permission_paths: BTreeMap<String, String>,
}

fn validate_backup(backup: &ConfigurationBackup) -> Result<(), String> {
    if backup.format != FORMAT || backup.version != FORMAT_VERSION {
        return Err("Unsupported AppBento configuration format or version".into());
    }
    let home = Path::new(&backup.home_directory);
    if !home.is_absolute() || home == Path::new("/") {
        return Err("Invalid backup home directory".into());
    }
    let mut ids = HashSet::new();
    for entry in &backup.apps {
        if entry.app.id.is_empty() || !ids.insert(&entry.app.id) {
            return Err("Backup contains empty or duplicate app identities".into());
        }
        if let Some(bytes) = &entry.icon {
            if bytes.len() > ICON_LIMIT {
                return Err("Backup icon exceeds the 5 MiB size limit".into());
            }
            crate::icons::image_format(bytes)?;
        }
        if entry.app.icon_path.is_some() && entry.icon.is_none() {
            return Err("Backup is missing an app icon".into());
        }
        if let Some(config) = &entry.app.update_config {
            crate::update_sources::validate_config(config)?;
        }
    }
    Ok(())
}

fn remap_path(value: &str, old_home: &str, new_home: &Path) -> String {
    Path::new(value)
        .strip_prefix(old_home)
        .map(|relative| new_home.join(relative).to_string_lossy().into_owned())
        .unwrap_or_else(|_| value.to_owned())
}

fn preview(backup: ConfigurationBackup, home: &Path) -> Result<ImportPreview, String> {
    validate_backup(&backup)?;
    let remap = |value: &str| remap_path(value, &backup.home_directory, home);
    Ok(ImportPreview {
        settings: Settings {
            managed_dir: remap(&backup.settings.managed_dir),
            scan_dirs: backup
                .settings
                .scan_dirs
                .iter()
                .map(|value| remap(value))
                .collect(),
        },
        app_paths: backup
            .apps
            .iter()
            .map(|entry| (entry.app.id.clone(), remap(&entry.app.file_path)))
            .collect(),
        permission_paths: backup
            .apps
            .iter()
            .filter_map(|entry| entry.app.launch_config.as_ref())
            .flat_map(|config| &config.permissions.paths)
            .map(|permission| (permission.path.clone(), remap(&permission.path)))
            .collect(),
        backup,
    })
}

fn read_backup(path: &Path) -> Result<ConfigurationBackup, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(NONBLOCKING | NO_FOLLOW)
        .open(path)
        .map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Select a regular configuration file".into());
    }
    let mut bytes = Vec::new();
    file.take(BACKUP_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > BACKUP_LIMIT {
        return Err("Configuration exceeds the 128 MiB size limit".into());
    }
    let backup = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid configuration file: {error}"))?;
    validate_backup(&backup)?;
    Ok(backup)
}

fn export_at(
    library_path: &Path,
    preferences: Preferences,
    home: &Path,
) -> Result<ConfigurationBackup, String> {
    persistence::with_repository(library_path, |repository: &mut Repository<Library>| {
        crate::lifecycle::recovery::recover(repository)?;
        let parent = library_path.parent().ok_or("invalid library path")?;
        let mut apps = repository.state.values().cloned().collect::<Vec<_>>();
        apps.sort_by(|first, second| first.id.cmp(&second.id));
        let entries = apps
            .into_iter()
            .map(|app| {
                let icon = app
                    .icon_path
                    .as_deref()
                    .map(|path| {
                        // Check ownership and bounded image contents using the existing icon reader.
                        crate::icons::data_url(&parent.join("icons"), path)?;
                        crate::icons::read_image(Path::new(path))
                    })
                    .transpose()?;
                Ok(AppBackup { app, icon })
            })
            .collect::<Result<_, String>>()?;
        Ok(ConfigurationBackup {
            format: FORMAT.into(),
            version: FORMAT_VERSION,
            home_directory: home.to_string_lossy().into_owned(),
            settings: persistence::read(&parent.join("settings.json"))?,
            preferences,
            apps: entries,
        })
    })
}

fn restore_at(
    library_path: &Path,
    desktop_directory: &Path,
    home: &Path,
    request: RestoreRequest,
) -> Result<Preferences, String> {
    validate_backup(&request.backup)?;
    let settings = crate::settings::validate(request.settings)?;
    persistence::with_repository(library_path, |repository: &mut Repository<Library>| {
        crate::lifecycle::recovery::recover(repository)?;
        let parent = library_path.parent().ok_or("invalid library path")?;
        let mut library = repository.state.clone();
        let mut changes = Vec::new();
        let mut used_paths = HashSet::new();
        let mut identities = BTreeMap::new();
        for entry in &request.backup.apps {
            let selected = request
                .app_paths
                .get(&entry.app.id)
                .ok_or_else(|| format!("Locate the AppImage for {}", entry.app.name))?;
            let (app, app_changes) = prepare_app(
                entry,
                selected,
                &request.backup.home_directory,
                home,
                parent,
                desktop_directory,
                &library,
                &request.permission_paths,
            )?;
            if !used_paths.insert(app.file_path.clone()) {
                return Err("Each app must use a different AppImage file".into());
            }
            identities.insert(entry.app.id.clone(), app.id.clone());
            library.insert(app.id.clone(), app);
            changes.extend(app_changes);
        }
        changes.push(FileChange::Write {
            path: parent.join("settings.json"),
            bytes: serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?,
        });
        changes.push(FileChange::Write {
            path: library_path.to_owned(),
            bytes: serde_json::to_vec_pretty(&library).map_err(|error| error.to_string())?,
        });
        let preferences = remap_preferences(request.backup.preferences, &identities);
        changes.push(FileChange::Write {
            path: parent.join("restored-preferences.json"),
            bytes: serde_json::to_vec(&preferences).map_err(|error| error.to_string())?,
        });
        persistence::commit_file_batch(parent, changes)?;
        Ok(preferences)
    })
}

fn prepare_app(
    entry: &AppBackup,
    selected: &str,
    old_home: &str,
    home: &Path,
    parent: &Path,
    desktop_directory: &Path,
    library: &Library,
    permission_paths: &BTreeMap<String, String>,
) -> Result<(InstalledApp, Vec<FileChange>), String> {
    crate::lifecycle::open_appimage(Path::new(selected))?;
    let path = fs::canonicalize(selected).map_err(|error| error.to_string())?;
    let file = crate::lifecycle::open_appimage(&path)?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    let mut app = entry.app.clone();
    app.file_path = path.to_str().ok_or("AppImage path must be UTF-8")?.into();
    app.id = library
        .values()
        .find(|existing| existing.file_path == app.file_path)
        .map(|existing| existing.id.clone())
        .unwrap_or_else(|| app.file_path.clone());
    app.desktop_id = library
        .get(&app.id)
        .and_then(|existing| existing.desktop_id.clone());
    if library
        .get(&app.id)
        .is_some_and(|existing| existing.file_path != app.file_path)
    {
        return Err("Restored app identity conflicts with an existing app".into());
    }
    app.file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("invalid filename")?
        .into();
    app.source_path = entry
        .app
        .source_path
        .as_deref()
        .map(|value| remap_path(value, old_home, home));
    app.size_bytes = metadata.len();
    app.executable = entry.app.executable;
    let digest = crate::sandbox::appimage_digest(&path)?;
    app.reviewed_digest = entry
        .app
        .reviewed_digest
        .clone()
        .filter(|reviewed| *reviewed == digest);
    if let Some(config) = &mut app.launch_config {
        for argument in &mut config.arguments {
            *argument = remap_path(argument, old_home, home);
        }
        for value in config.environment.values_mut() {
            *value = remap_path(value, old_home, home);
        }
        for permission in &mut config.permissions.paths {
            permission.path = permission_paths
                .get(&permission.path)
                .cloned()
                .unwrap_or_else(|| remap_path(&permission.path, old_home, home));
        }
        *config = crate::sandbox::validate_config(config, parent).map_err(|error| {
            format!(
                "{}: {error}. Create or relocate its permission folders before restoring.",
                app.name
            )
        })?;
    }
    let mut changes = Vec::new();
    app.icon_path = entry
        .icon
        .as_ref()
        .map(|bytes| {
            let (extension, _) = crate::icons::image_format(bytes)?;
            let path = parent
                .join("icons")
                .join(format!("{:x}.{extension}", Sha256::digest(bytes)));
            changes.push(FileChange::Write {
                path: path.clone(),
                bytes: bytes.clone(),
            });
            Ok::<_, String>(path.to_string_lossy().into_owned())
        })
        .transpose()?;
    let mode = metadata.permissions().mode() & 0o777;
    changes.push(FileChange::SetPermissions {
        path: path.clone(),
        mode: if app.executable {
            mode | 0o111
        } else {
            mode & !0o111
        },
    });
    let desktop = prepare_desktop(&app, desktop_directory, library)?;
    app.desktop_id = desktop.as_ref().map(|(id, _)| id.clone());
    if let Some((_, change)) = desktop {
        changes.push(change);
    }
    Ok((app, changes))
}

fn prepare_desktop(
    app: &InstalledApp,
    desktop_directory: &Path,
    library: &Library,
) -> Result<Option<(String, FileChange)>, String> {
    if app.desktop_id.is_none() {
        return Ok(None);
    }
    let id = app
        .desktop_id
        .clone()
        .ok_or("desktop entry id unavailable")?;
    let destination = crate::lifecycle::desktop::entry_path(desktop_directory, app)?
        .ok_or("desktop entry id unavailable")?;
    if !destination
        .try_exists()
        .map_err(|error| error.to_string())?
    {
        return Ok(None);
    }
    if destination.exists() {
        let metadata = fs::symlink_metadata(&destination).map_err(|error| error.to_string())?;
        let is_known = library.values().any(|existing| {
            existing.file_path == app.file_path && existing.desktop_id.as_deref() == Some(&id)
        });
        let content = fs::read_to_string(&destination).map_err(|error| error.to_string())?;
        let is_owned = content
            .lines()
            .any(|line| line == "X-AppBento-Managed=true");
        if !metadata.file_type().is_file() || !is_known || !is_owned {
            return Err(
                "A desktop entry already exists and its ownership cannot be verified".into(),
            );
        }
    }
    let content = crate::lifecycle::desktop::content_for_entry(app)?;
    Ok(Some((
        id,
        FileChange::Write {
            path: destination,
            bytes: content.into_bytes(),
        },
    )))
}

fn remap_preferences(
    preferences: Preferences,
    identities: &BTreeMap<String, String>,
) -> Preferences {
    let remap = |id: &String| identities.get(id).cloned().unwrap_or_else(|| id.clone());
    Preferences {
        theme: preferences.theme,
        layout: Layout {
            ungrouped: preferences.layout.ungrouped.iter().map(remap).collect(),
            groups: preferences
                .layout
                .groups
                .into_iter()
                .map(|group| Group {
                    app_ids: group.app_ids.iter().map(remap).collect(),
                    ..group
                })
                .collect(),
        },
        display_names: preferences
            .display_names
            .into_iter()
            .map(|(id, name)| (remap(&id), name))
            .collect(),
    }
}

#[tauri::command]
pub async fn configuration_export(path: String, preferences: Preferences) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let backup = export_at(
            &persistence::data_path("library.json")?,
            preferences,
            &dirs::home_dir().ok_or("home directory unavailable")?,
        )?;
        let bytes = serde_json::to_vec_pretty(&backup).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > BACKUP_LIMIT {
            return Err("Configuration exceeds the 128 MiB size limit".into());
        }
        persistence::write_atomic(Path::new(&path), &bytes).map_err(|error| error.message)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn configuration_read(path: String) -> Result<ImportPreview, String> {
    tokio::task::spawn_blocking(move || {
        preview(
            read_backup(Path::new(&path))?,
            &dirs::home_dir().ok_or("home directory unavailable")?,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn configuration_restore(request: RestoreRequest) -> Result<Preferences, String> {
    tokio::task::spawn_blocking(move || {
        restore_at(
            &persistence::data_path("library.json")?,
            &dirs::data_dir()
                .ok_or("data directory unavailable")?
                .join("applications"),
            &dirs::home_dir().ok_or("home directory unavailable")?,
            request,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn configuration_preferences(completed: bool) -> Result<Option<Preferences>, String> {
    tokio::task::spawn_blocking(move || {
        persistence::with_repository(
            &persistence::data_path("library.json")?,
            |_: &mut Repository<Library>| {
                let path = persistence::data_path("restored-preferences.json")?;
                if completed {
                    persistence::write_atomic(&path, b"null").map_err(|error| error.message)?;
                    return Ok(None);
                }
                persistence::read(&path)
            },
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
#[path = "configuration_tests.rs"]
mod tests;
