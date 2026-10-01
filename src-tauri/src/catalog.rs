//! Validated catalog embedded at compile time; launch directories cannot override it.
use std::collections::HashSet;

use crate::models::{CatalogEntry, UpdateStrategy};
use regex::Regex;

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));

pub fn load_catalog() -> Result<Vec<CatalogEntry>, String> {
    parse_catalog(BUNDLED_CATALOG)
}

fn parse_catalog(files: &[(&str, &str)]) -> Result<Vec<CatalogEntry>, String> {
    let mut entries = Vec::new();
    let mut ids = HashSet::new();
    for (name, text) in files {
        let entry: CatalogEntry =
            toml::from_str(text).map_err(|error| format!("{name}: {error}"))?;
        validate_entry(&entry).map_err(|error| format!("{name}: {error}"))?;
        if *name != format!("{}.toml", entry.app.id) || !ids.insert(entry.app.id.clone()) {
            return Err(format!("{name}: duplicate or mismatched app id"));
        }
        entries.push(entry);
    }
    entries.sort_by_key(|entry| entry.app.name.to_lowercase());
    Ok(entries)
}

fn validate_entry(entry: &CatalogEntry) -> Result<(), String> {
    if entry.app.id.is_empty()
        || !entry
            .app
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("invalid app id".into());
    }
    if entry.app.name.trim().is_empty()
        || entry.app.summary.trim().is_empty()
        || entry.match_filenames.is_empty()
    {
        return Err("name, summary and filename patterns are required".into());
    }
    for url in std::iter::once(&entry.app.website)
        .chain(entry.app.repo.iter())
        .chain(entry.app.docs_url.iter())
        .chain(std::iter::once(&entry.branding.logo))
        .chain(entry.branding.logo_dark.iter())
        .chain(entry.branding.cover.iter())
        .chain(entry.branding.screenshots.iter())
    {
        let parsed = reqwest::Url::parse(url).map_err(|error| error.to_string())?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err("catalog links and artwork must use HTTPS without credentials".into());
        }
    }
    let version = Regex::new(&entry.version_regex).map_err(|error| error.to_string())?;
    if !version.capture_names().any(|name| name == Some("version")) {
        return Err("version_regex requires a named version capture".into());
    }
    match entry.update.strategy {
        Some(UpdateStrategy::GithubRelease) => {
            let config = entry
                .update
                .github
                .as_ref()
                .ok_or("github update configuration missing")?;
            crate::github::validate_config(config)?;
        }
        Some(UpdateStrategy::Manual) => {}
        _ => return Err("unsupported catalog update strategy".into()),
    }
    Ok(())
}

pub fn version_from_filename(
    entries: &[CatalogEntry],
    catalog_id: &str,
    file_name: &str,
) -> Option<String> {
    let entry = entries.iter().find(|entry| entry.app.id == catalog_id)?;
    if !entry
        .match_filenames
        .iter()
        .any(|pattern| glob_match(&pattern.to_lowercase(), &file_name.to_lowercase()))
    {
        return None;
    }
    let expression = Regex::new(&entry.version_regex).ok()?;
    let captures = expression.captures(file_name)?;
    let version = captures.name("version")?.as_str();
    crate::updates::parse_version(version).map(|_| version.to_string())
}

/// Match filenames against explicit catalog patterns (`*` wildcards supported).
pub fn match_catalog(entries: &[CatalogEntry], file_name: &str) -> Option<String> {
    let lower = file_name.to_lowercase();
    for entry in entries {
        for pattern in &entry.match_filenames {
            if glob_match(&pattern.to_lowercase(), &lower) {
                return Some(entry.app.id.clone());
            }
        }
    }
    None
}

fn glob_match(pattern: &str, text: &str) -> bool {
    // `*`-only glob over bytes.
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut match_idx) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some(pi + 1);
            match_idx = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(rest) = star {
            match_idx += 1;
            ti = match_idx;
            pi = rest;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_filename_patterns_are_loaded() {
        let entries = load_catalog().unwrap();
        assert!(entries
            .iter()
            .all(|entry| !entry.match_filenames.is_empty()));
        assert_eq!(
            match_catalog(&entries, "nvim-linux-x86_64.appimage"),
            Some("neovim".into())
        );
        assert_eq!(
            match_catalog(&entries, "Nextcloud-3.16.0.AppImage"),
            Some("nextcloud-desktop".into())
        );
    }

    #[test]
    fn filename_versions_exclude_architecture_and_preserve_unknowns() {
        let entries = load_catalog().unwrap();
        for filename in [
            "Obsidian-1.5.3-x86_64.AppImage",
            "Obsidian-1.5.3-amd64.AppImage",
            "Obsidian-1.5.3.AppImage",
        ] {
            assert_eq!(
                version_from_filename(&entries, "obsidian", filename),
                Some("1.5.3".into())
            );
        }
        assert_eq!(
            version_from_filename(&entries, "obsidian", "Obsidian-latest.AppImage"),
            None
        );
        assert_eq!(
            version_from_filename(&entries, "neovim", "nvim-linux-x86_64.appimage"),
            None
        );
        assert_eq!(match_catalog(&entries, "fakeobsidian.AppImage"), None);
    }

    #[test]
    fn rejects_unknown_fields_duplicate_ids_and_invalid_configurations() {
        let (name, text) = BUNDLED_CATALOG
            .iter()
            .find(|(name, _)| *name == "obsidian.toml")
            .unwrap();
        let unexpected = format!("{text}\n[unexpected]\nvalue = true\n");
        assert!(parse_catalog(&[(name, &unexpected)]).is_err());
        assert!(parse_catalog(&[(name, text), (name, text)]).is_err());
        let mut entry: CatalogEntry = toml::from_str(text).unwrap();
        entry.version_regex = "[".into();
        assert!(validate_entry(&entry).is_err());
        entry.version_regex = "(.*)".into();
        assert!(validate_entry(&entry).is_err());
        entry.version_regex = "(?P<version>.*)".into();
        entry.app.website = "http://unsafe.example".into();
        assert!(validate_entry(&entry).is_err());
    }

    #[test]
    fn glob_basics() {
        assert!(glob_match("obsidian-*.appimage", "obsidian-1.5.3.appimage"));
        assert!(!glob_match("obsidian-*.appimage", "joplin-1.5.3.appimage"));
        assert!(glob_match("*.appimage", "foo.appimage"));
    }
}
