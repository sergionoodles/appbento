//! Derive an app's display name and version from embedded metadata and its filename.
use std::sync::LazyLock;

use regex::Regex;

use crate::embedded_icon::EmbeddedIdentity;

const MAX_NAME_LENGTH: usize = 80;
const MAX_VERSION_LENGTH: usize = 64;
const APPIMAGE_EXTENSION: &str = ".appimage";

/// Words that describe the download (packaging, platform, architecture) rather than the app.
static PACKAGING_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:appimage|portable|linux|x64|x86_64|amd64|aarch64|arm64|armhf|i[3-6]86|64-?bit|ubuntu[\d.]*)$")
        .expect("valid regex")
});
static VERSION_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[vV]?\d+(?:\.\d+)+(?:[-+~][0-9A-Za-z.]+)?$").expect("valid regex")
});
// Removed before separators become spaces so "x86_64" does not split into "x86 64".
static ARCHITECTURE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[-_.](?:x86[-_]64|amd64|aarch64|arm64|armhf|i[3-6]86)").expect("valid regex")
});
static TRAILING_BRACKETS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*[(\[][^()\[\]]*[)\]]\s*$").expect("valid regex"));
static FILE_NAME_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^0-9a-z])v?(\d+(?:\.\d+)+)(?:[^0-9]|$)").expect("valid regex")
});

#[derive(Debug, PartialEq, Eq)]
pub struct AppIdentity {
    pub name: String,
    pub version: Option<String>,
}

pub fn resolve(embedded: &EmbeddedIdentity, file_name: &str) -> AppIdentity {
    let name = embedded
        .name
        .as_deref()
        .map(clean_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| name_from_file_name(file_name));
    let version = embedded
        .version
        .as_deref()
        .and_then(normalize_version)
        .or_else(|| version_from_file_name(file_name));
    AppIdentity { name, version }
}

/// Drops packaging qualifiers, version numbers and a trailing bracketed note from a name,
/// e.g. "Audacity 4 Portable" becomes "Audacity 4" and "Tool (x64)" becomes "Tool".
pub fn clean_name(raw: &str) -> String {
    let printable: String = raw
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    let without_note = TRAILING_BRACKETS.replace(printable.trim(), "");
    let cleaned = without_note
        .split_whitespace()
        .filter(|word| !PACKAGING_WORD.is_match(word) && !VERSION_WORD.is_match(word))
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = cleaned.trim_matches(|character: char| {
        character.is_whitespace() || matches!(character, '-' | '–' | '—' | ':' | ',')
    });
    let name = if trimmed.is_empty() {
        printable.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        trimmed.to_owned()
    };
    name.chars().take(MAX_NAME_LENGTH).collect()
}

pub fn name_from_file_name(file_name: &str) -> String {
    let stem = strip_appimage_extension(file_name);
    let without_architecture = ARCHITECTURE_SUFFIX.replace_all(stem, "");
    let spaced = without_architecture.replace(['-', '_'], " ");
    let cleaned = clean_name(&spaced);
    if cleaned.is_empty() {
        return stem.to_owned();
    }
    cleaned
}

pub fn version_from_file_name(file_name: &str) -> Option<String> {
    FILE_NAME_VERSION
        .captures(strip_appimage_extension(file_name))
        .and_then(|captures| captures.get(1))
        .and_then(|version| normalize_version(version.as_str()))
}

/// Whether `name` is still what older releases derived from the filename, so it is safe to
/// replace with embedded metadata without overwriting anything meaningful.
pub fn is_legacy_file_name_default(name: &str, file_name: &str) -> bool {
    name == file_name
        .trim_end_matches(".AppImage")
        .replace(['-', '_'], " ")
}

fn normalize_version(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let version = trimmed
        .strip_prefix(['v', 'V'])
        .filter(|rest| rest.starts_with(|character: char| character.is_ascii_digit()))
        .unwrap_or(trimmed);
    let is_plausible = !version.is_empty()
        && version.len() <= MAX_VERSION_LENGTH
        && version.chars().any(|character| character.is_ascii_digit())
        && version
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-+_~".contains(character));
    is_plausible.then(|| version.to_owned())
}

fn strip_appimage_extension(file_name: &str) -> &str {
    let split = file_name.len().saturating_sub(APPIMAGE_EXTENSION.len());
    match file_name.get(split..) {
        Some(extension) if extension.eq_ignore_ascii_case(APPIMAGE_EXTENSION) => {
            &file_name[..split]
        }
        _ => file_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embedded(name: Option<&str>, version: Option<&str>) -> EmbeddedIdentity {
        EmbeddedIdentity {
            name: name.map(str::to_owned),
            version: version.map(str::to_owned),
        }
    }

    #[test]
    fn prefers_embedded_name_and_version() {
        assert_eq!(
            resolve(
                &embedded(Some("Beekeeper Studio"), Some("6.1.4")),
                "Beekeeper-Studio-6.1.4.AppImage"
            ),
            AppIdentity {
                name: "Beekeeper Studio".into(),
                version: Some("6.1.4".into())
            }
        );
    }

    #[test]
    fn falls_back_to_the_file_name_for_missing_metadata() {
        assert_eq!(
            resolve(
                &embedded(Some("OrcaSlicer"), None),
                "OrcaSlicer_Linux_AppImage_Ubuntu2404_V2.4.2.AppImage"
            ),
            AppIdentity {
                name: "OrcaSlicer".into(),
                version: Some("2.4.2".into())
            }
        );
        assert_eq!(
            resolve(
                &embedded(None, None),
                "audacity-linux-4.0.1-x86_64.AppImage"
            ),
            AppIdentity {
                name: "audacity".into(),
                version: Some("4.0.1".into())
            }
        );
        assert_eq!(
            resolve(&embedded(None, None), "orca.appimage"),
            AppIdentity {
                name: "orca".into(),
                version: None
            }
        );
    }

    #[test]
    fn cleans_packaging_noise_from_names() {
        assert_eq!(clean_name("Audacity 4 Portable"), "Audacity 4");
        assert_eq!(clean_name("Tool (x64)"), "Tool");
        assert_eq!(clean_name("Editor 1.2.3 - Linux"), "Editor");
        assert_eq!(
            clean_name("Visual Studio Code - Insiders"),
            "Visual Studio Code - Insiders"
        );
        assert_eq!(
            name_from_file_name("skillshard-v0.4.1-linux-x86_64.AppImage"),
            "skillshard"
        );
        assert_eq!(
            name_from_file_name("Recordly-linux-x64.AppImage"),
            "Recordly"
        );
    }

    #[test]
    fn keeps_a_name_made_only_of_noise() {
        assert_eq!(clean_name("Portable"), "Portable");
        assert_eq!(name_from_file_name("linux.AppImage"), "linux");
    }

    #[test]
    fn rejects_implausible_versions() {
        assert_eq!(
            resolve(&embedded(None, Some("v1.4.218")), "x.AppImage").version,
            Some("1.4.218".into())
        );
        assert_eq!(
            resolve(&embedded(None, Some("continuous")), "x.AppImage").version,
            None
        );
        assert_eq!(
            resolve(&embedded(None, Some("1.0; rm -rf")), "x.AppImage").version,
            None
        );
        assert_eq!(
            version_from_file_name("tool-x86_64-Ubuntu2404.AppImage"),
            None
        );
    }

    #[test]
    fn recognizes_names_derived_by_older_releases() {
        assert!(is_legacy_file_name_default(
            "audacity linux 4.0.1 x86 64",
            "audacity-linux-4.0.1-x86_64.AppImage"
        ));
        assert!(!is_legacy_file_name_default(
            "Audacity",
            "audacity-linux-4.0.1-x86_64.AppImage"
        ));
    }
}
