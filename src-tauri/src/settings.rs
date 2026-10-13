//! Persisted and validated scan / integration folders.
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::persistence;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub scan_dirs: Vec<String>,
    pub managed_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan_dirs: Vec::new(),
            managed_dir: persistence::data_path("apps")
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

pub fn normalize_directory(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    let expanded = if trimmed == "~" || trimmed.starts_with("~/") {
        dirs::home_dir()
            .ok_or("home directory unavailable")?
            .join(trimmed.trim_start_matches('~').trim_start_matches('/'))
    } else {
        PathBuf::from(trimmed)
    };
    if !expanded.is_absolute() || trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return Err(
            "folders must be absolute paths (or ~/ paths) without control characters".into(),
        );
    }
    let mut normalized = PathBuf::new();
    for component in expanded.components() {
        match component {
            Component::ParentDir => {
                return Err("folder paths cannot contain parent traversal".into())
            }
            Component::CurDir => {}
            component => normalized.push(component.as_os_str()),
        }
    }
    if normalized == Path::new("/") {
        return Err("the filesystem root cannot be used as an application folder".into());
    }
    if normalized.exists() && !normalized.is_dir() {
        return Err("application folder must be a directory".into());
    }
    let mut existing = normalized.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        missing.push(
            existing
                .file_name()
                .ok_or("invalid folder path")?
                .to_owned(),
        );
        existing = existing.parent().ok_or("invalid folder path")?;
    }
    let mut canonical = std::fs::canonicalize(existing).map_err(|error| error.to_string())?;
    for component in missing.into_iter().rev() {
        canonical.push(component);
    }
    if canonical == Path::new("/") {
        return Err("the filesystem root cannot be used as an application folder".into());
    }
    Ok(canonical)
}

pub(crate) fn validate(settings: Settings) -> Result<Settings, String> {
    Ok(Settings {
        managed_dir: normalize_directory(&settings.managed_dir)?
            .to_str()
            .ok_or("folder path must be UTF-8")?
            .to_owned(),
        scan_dirs: settings
            .scan_dirs
            .iter()
            .map(|path| {
                normalize_directory(path).and_then(|path| {
                    path.to_str()
                        .map(str::to_owned)
                        .ok_or("folder path must be UTF-8".into())
                })
            })
            .collect::<Result<_, _>>()?,
    })
}

pub fn load() -> Result<Settings, String> {
    validate(persistence::load(&persistence::data_path(
        "settings.json",
    )?)?)
}

pub fn merge_scan_dirs(extra: Vec<String>, stored: Vec<String>) -> Result<Vec<PathBuf>, String> {
    let mut output = Vec::new();
    for raw in extra.into_iter().chain(stored) {
        if raw.trim().is_empty() {
            continue;
        }
        let path = normalize_directory(&raw)?;
        if !output.contains(&path) {
            output.push(path);
        }
    }
    Ok(output)
}

#[tauri::command]
pub async fn settings_get() -> Result<Settings, String> {
    tokio::task::spawn_blocking(load)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn settings_set(
    scan_dirs: Option<Vec<String>>,
    managed_dir: Option<String>,
) -> Result<Settings, String> {
    tokio::task::spawn_blocking(move || {
        persistence::mutate(
            &persistence::data_path("settings.json")?,
            |settings: &mut Settings| {
                let next = validate(Settings {
                    scan_dirs: scan_dirs
                        .unwrap_or_else(|| settings.scan_dirs.clone())
                        .into_iter()
                        .filter(|path| !path.trim().is_empty())
                        .collect(),
                    managed_dir: managed_dir.unwrap_or_else(|| settings.managed_dir.clone()),
                })?;
                *settings = next.clone();
                Ok(next)
            },
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_managed_folder_is_beside_settings_file() {
        assert_eq!(
            PathBuf::from(Settings::default().managed_dir),
            persistence::data_path("settings.json")
                .unwrap()
                .parent()
                .unwrap()
                .join("apps")
        );
    }

    #[test]
    fn merge_dedupes_and_skips_blanks() {
        assert_eq!(
            merge_scan_dirs(
                vec!["/b".into(), " ".into()],
                vec!["/b".into(), "/c".into()]
            )
            .unwrap(),
            vec![PathBuf::from("/b"), "/c".into()]
        );
    }

    #[test]
    fn normalizes_home_and_rejects_relative_and_control_paths() {
        assert_eq!(
            normalize_directory("~/Applications").unwrap(),
            dirs::home_dir().unwrap().join("Applications")
        );
        assert!(normalize_directory("relative").is_err());
        assert!(normalize_directory("/tmp/new\nfolder").is_err());
        assert!(normalize_directory("/").is_err());
    }
}
