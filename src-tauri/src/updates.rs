//! Update checks are read-only: resolve manual downloads without changing installed files.
use std::cmp::Ordering;

use futures::{stream, StreamExt};

use crate::github::{self, ResolvedUpdate};
use crate::models::{
    CatalogEntry, InstalledApp, UpdateCheckError, UpdateCheckResult, UpdateInfo, UpdateReport,
    UpdateStatus, UpdateStrategy,
};

const MAX_CONCURRENT_CHECKS: usize = 4;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Version {
    numbers: Vec<u64>,
    prerelease: Vec<String>,
}

pub(crate) fn parse_version(text: &str) -> Option<Version> {
    let normalized = text
        .strip_prefix('v')
        .or_else(|| text.strip_prefix('V'))
        .unwrap_or(text);
    let (version, metadata) = normalized
        .split_once('+')
        .map_or((normalized, None), |(version, metadata)| {
            (version, Some(metadata))
        });
    if metadata.is_some_and(|value| !valid_identifiers(value)) {
        return None;
    }
    let (numbers, prerelease) = version.split_once('-').unwrap_or((version, ""));
    if numbers.is_empty()
        || !numbers
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return None;
    }
    let numbers: Vec<u64> = numbers
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if !(2..=4).contains(&numbers.len())
        || (version.contains('-') && !valid_identifiers(prerelease))
    {
        return None;
    }
    Some(Version {
        numbers,
        prerelease: if prerelease.is_empty() {
            Vec::new()
        } else {
            prerelease.split('.').map(str::to_string).collect()
        },
    })
}

fn valid_identifiers(text: &str) -> bool {
    text.split('.').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn compare_versions(left: &Version, right: &Version) -> Ordering {
    for index in 0..left.numbers.len().max(right.numbers.len()) {
        let comparison = left
            .numbers
            .get(index)
            .unwrap_or(&0)
            .cmp(right.numbers.get(index).unwrap_or(&0));
        if comparison != Ordering::Equal {
            return comparison;
        }
    }
    match (left.prerelease.is_empty(), right.prerelease.is_empty()) {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (left_part, right_part) in left.prerelease.iter().zip(&right.prerelease) {
        let comparison = match (left_part.parse::<u64>(), right_part.parse::<u64>()) {
            (Ok(left_number), Ok(right_number)) => left_number.cmp(&right_number),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => left_part.cmp(right_part),
        };
        if comparison != Ordering::Equal {
            return comparison;
        }
    }
    left.prerelease.len().cmp(&right.prerelease.len())
}

fn failure(app_id: &str, status: UpdateStatus, message: String) -> UpdateCheckResult {
    UpdateCheckResult {
        app_id: app_id.into(),
        status,
        update: None,
        error: Some(message),
    }
}

pub(crate) fn resolve_update(app: &InstalledApp, resolved: ResolvedUpdate) -> UpdateCheckResult {
    let Some(latest) = parse_version(&resolved.latest) else {
        return failure(
            &app.id,
            UpdateStatus::Error,
            "release version is not comparable".into(),
        );
    };
    let current = app.version_current.as_deref().and_then(parse_version);
    let status = match current {
        Some(current) if compare_versions(&latest, &current) == Ordering::Greater => {
            UpdateStatus::Available
        }
        Some(_) => UpdateStatus::UpToDate,
        None => UpdateStatus::UnknownCurrent,
    };
    let update = if status == UpdateStatus::UpToDate {
        None
    } else {
        Some(UpdateInfo {
            app_id: app.id.clone(),
            status: status.clone(),
            current: app.version_current.clone(),
            latest: resolved.latest,
            download_url: resolved.download_url,
            asset_name: resolved.asset_name,
            published_at: resolved.published_at,
            size_bytes: resolved.size_bytes,
        })
    };
    UpdateCheckResult {
        app_id: app.id.clone(),
        status,
        update,
        error: None,
    }
}

pub async fn check_app(app: &InstalledApp, entries: &[CatalogEntry]) -> UpdateCheckResult {
    let entry = app
        .catalog_id
        .as_ref()
        .and_then(|id| entries.iter().find(|entry| &entry.app.id == id));
    let Some(entry) = entry else {
        return failure(
            &app.id,
            UpdateStatus::Unsupported,
            "no catalog update source for this app".into(),
        );
    };
    if !matches!(entry.update.strategy, Some(UpdateStrategy::GithubRelease)) {
        return failure(
            &app.id,
            UpdateStatus::Unsupported,
            "automatic checks are unsupported for this update strategy".into(),
        );
    }
    let Some(config) = entry.update.github.as_ref() else {
        return failure(
            &app.id,
            UpdateStatus::Error,
            "GitHub update configuration missing".into(),
        );
    };
    if config.architecture != std::env::consts::ARCH {
        return failure(
            &app.id,
            UpdateStatus::Unsupported,
            format!("no catalog assets for {}", std::env::consts::ARCH),
        );
    }
    match github::check_github(config).await {
        Ok(resolved) => resolve_update(app, resolved),
        Err(message) => failure(&app.id, UpdateStatus::Error, message),
    }
}

pub async fn check_all(apps: Vec<InstalledApp>, entries: Vec<CatalogEntry>) -> UpdateReport {
    let entries = std::sync::Arc::new(entries);
    let results = stream::iter(apps)
        .map(move |app| {
            let entries = entries.clone();
            async move { check_app(&app, &entries).await }
        })
        .buffer_unordered(MAX_CONCURRENT_CHECKS)
        .collect::<Vec<_>>()
        .await;
    report_results(results)
}

fn report_results(results: Vec<UpdateCheckResult>) -> UpdateReport {
    let mut report = UpdateReport::default();
    for result in results {
        match result.status {
            UpdateStatus::Error => report.errors.push(UpdateCheckError {
                app_id: result.app_id,
                message: result.error.unwrap_or_else(|| "update check failed".into()),
            }),
            UpdateStatus::Unsupported => report.unsupported_count += 1,
            UpdateStatus::UnknownCurrent => {
                report.checked_count += 1;
                report.unknown_versions.push(result.app_id);
            }
            _ => report.checked_count += 1,
        }
        if let Some(update) = result.update {
            report.updates.push(update);
        }
    }
    report
        .updates
        .sort_by(|left, right| left.app_id.cmp(&right.app_id));
    report
        .errors
        .sort_by(|left, right| left.app_id.cmp(&right.app_id));
    report.unknown_versions.sort();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        parse_version(text).unwrap()
    }

    #[test]
    fn compares_numeric_versions_and_prevents_downgrades() {
        assert_eq!(
            compare_versions(&version("1.10.0"), &version("1.9.0")),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions(&version("v1.2"), &version("1.2.0+build")),
            Ordering::Equal
        );
        assert_eq!(
            compare_versions(&version("1.2.0"), &version("2.0.0")),
            Ordering::Less
        );
        assert_eq!(
            compare_versions(&version("1.2.0-rc.10"), &version("1.2.0-rc.2")),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions(&version("1.2.0"), &version("1.2.0-rc.10")),
            Ordering::Greater
        );
        assert!(parse_version("latest").is_none());
        assert!(parse_version("1.2.0-").is_none());
    }

    fn app(current: Option<&str>) -> InstalledApp {
        InstalledApp {
            id: "app".into(),
            name: "App".into(),
            file_path: "/tmp/App.AppImage".into(),
            source_path: None,
            file_name: "App.AppImage".into(),
            size_bytes: 0,
            executable: true,
            integrated: false,
            catalog_id: None,
            version_current: current.map(str::to_string),
            icon_path: None,
            desktop_id: None,
            launch_count: 0,
            last_launched: None,
            added_at: chrono::Utc::now(),
        }
    }

    fn release(latest: &str) -> ResolvedUpdate {
        ResolvedUpdate {
            latest: latest.into(),
            download_url: "https://github.com/owner/app/releases/download/v2.0/App.AppImage".into(),
            asset_name: "App.AppImage".into(),
            published_at: None,
            size_bytes: None,
        }
    }

    #[test]
    fn resolves_newer_current_unknown_and_malformed_versions() {
        let newer = resolve_update(&app(Some("1.9.0")), release("1.10.0"));
        assert_eq!(newer.status, UpdateStatus::Available);
        assert_eq!(newer.update.unwrap().latest, "1.10.0");
        let current = resolve_update(&app(Some("2.0.0")), release("1.10.0"));
        assert_eq!(current.status, UpdateStatus::UpToDate);
        assert!(current.update.is_none());
        let unknown = resolve_update(&app(None), release("1.10.0"));
        assert_eq!(unknown.status, UpdateStatus::UnknownCurrent);
        assert_eq!(unknown.update.unwrap().status, UpdateStatus::UnknownCurrent);
        assert_eq!(
            resolve_update(&app(Some("unparseable")), release("1.10.0")).status,
            UpdateStatus::UnknownCurrent
        );
        assert_eq!(
            resolve_update(&app(Some("1.0.0")), release("latest")).status,
            UpdateStatus::Error
        );
    }

    #[tokio::test]
    async fn unmatched_apps_report_unsupported_without_network() {
        let result = check_app(&app(None), &[]).await;
        assert_eq!(result.status, UpdateStatus::Unsupported);
        assert!(result.update.is_none());
        let report = check_all(vec![app(None)], vec![]).await;
        assert_eq!(report.unsupported_count, 1);
        assert_eq!(report.checked_count, 0);
    }

    #[test]
    fn reports_failures_and_unsupported_checks_separately() {
        let report = report_results(vec![
            failure("broken", UpdateStatus::Error, "rate limited".into()),
            failure("manual", UpdateStatus::Unsupported, "manual source".into()),
            UpdateCheckResult {
                app_id: "current".into(),
                status: UpdateStatus::UpToDate,
                update: None,
                error: None,
            },
            UpdateCheckResult {
                app_id: "unknown".into(),
                status: UpdateStatus::UnknownCurrent,
                update: None,
                error: None,
            },
        ]);
        assert_eq!(report.errors[0].message, "rate limited");
        assert_eq!(report.checked_count, 2);
        assert_eq!(report.unsupported_count, 1);
        assert_eq!(report.unknown_versions, vec!["unknown"]);
    }
}
