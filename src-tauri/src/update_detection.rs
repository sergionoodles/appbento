//! Detect the update source an AppImage declares, and decide when to apply it.
use std::path::Path;

use crate::models::{GithubUpdateConfig, InstalledApp, UpdateConfig, UpdateConfigOrigin};
use crate::simple_yaml::{is_ignorable, key_value};

const DEFAULT_TAG_REGEX: &str = "^v?(?P<version>.*)$";
const STABLE_CHANNEL: &str = "stable";
const PRERELEASE_CHANNEL: &str = "prerelease";

/// Update settings saved with an app, and who chose them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateSource {
    pub config: Option<UpdateConfig>,
    pub origin: Option<UpdateConfigOrigin>,
}

impl UpdateSource {
    pub(crate) fn of(app: &InstalledApp) -> Self {
        Self {
            config: app.update_config.clone(),
            origin: app.update_config_origin,
        }
    }

    fn detected(config: Option<UpdateConfig>) -> Self {
        let origin = config.is_some().then_some(UpdateConfigOrigin::Detected);
        Self { config, origin }
    }
}

/// Applies the detection rules after an import, rescan or update. `detect` runs only when the
/// rules call for it, and a failed detection keeps whatever was saved.
pub(crate) fn source_after_discovery(
    existing: Option<UpdateSource>,
    file_changed: bool,
    detect: impl FnOnce() -> Option<UpdateConfig>,
) -> UpdateSource {
    let Some(existing) = existing else {
        return UpdateSource::detected(detect());
    };
    match (existing.origin, existing.config.is_none()) {
        (Some(UpdateConfigOrigin::User), _) => existing,
        (None, true) => UpdateSource::detected(detect()),
        (Some(UpdateConfigOrigin::Detected), _) if file_changed => {
            detect().map_or(existing, |config| UpdateSource::detected(Some(config)))
        }
        _ => existing,
    }
}

/// Never fails: update detection must not block importing or scanning an app.
pub(crate) fn detect_logged(path: &Path) -> Option<UpdateConfig> {
    detect(path).unwrap_or_else(|error| {
        eprintln!(
            "update source detection failed for {}: {error}",
            path.display()
        );
        None
    })
}

fn detect(path: &Path) -> Result<Option<UpdateConfig>, String> {
    let upd_info = crate::lifecycle::open_appimage(path)
        .and_then(|mut file| crate::elf_sections::read_upd_info(&mut file));
    let manifest = crate::embedded_icon::read_app_update_manifest(path);
    let from_manifest = manifest
        .as_ref()
        .ok()
        .and_then(|text| text.as_deref())
        .and_then(config_from_app_update_yml);
    let from_upd_info = upd_info
        .as_ref()
        .ok()
        .and_then(|text| text.as_deref())
        .and_then(|text| config_from_upd_info(text, std::env::consts::ARCH));
    // The electron feed carries a version and SHA-512, so it wins over `.upd_info`.
    let detected = from_manifest
        .or(from_upd_info)
        .filter(|config| crate::update_sources::validate_config(config).is_ok());
    if detected.is_some() {
        return Ok(detected);
    }
    upd_info.and(manifest).map(|_| None)
}

fn is_repository_part(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().all(|character| character == '.')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        })
}

/// `*` is the only wildcard in AppImageUpdate patterns; everything else matches literally.
fn asset_regex_from_glob(pattern: &str) -> Option<String> {
    let glob = pattern.strip_suffix(".zsync").unwrap_or(pattern);
    if glob.is_empty() || glob.chars().any(char::is_control) {
        return None;
    }
    let body = glob
        .split('*')
        .map(escape_literal)
        .collect::<Vec<_>>()
        .join(".*");
    Some(format!("^{body}$"))
}

/// Unlike `regex::escape`, leaves `-` alone so the saved pattern stays readable.
fn escape_literal(text: &str) -> String {
    text.chars().fold(String::new(), |mut escaped, character| {
        if "\\.+*?()|[]{}^$".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
        escaped
    })
}

pub(crate) fn config_from_upd_info(info: &str, architecture: &str) -> Option<UpdateConfig> {
    let info = info.trim();
    if let Some(url) = info.strip_prefix("zsync|") {
        return url.starts_with("https://").then(|| UpdateConfig::Zsync {
            url: url.to_owned(),
        });
    }
    let parts: Vec<_> = info.split('|').collect();
    let ["gh-releases-zsync", owner, repo, tag, pattern] = parts.as_slice() else {
        return None;
    };
    let (channel, prerelease) = match *tag {
        "latest" => (STABLE_CHANNEL, false),
        "latest-pre" | "latest-all" => (PRERELEASE_CHANNEL, true),
        _ => return None,
    };
    if !is_repository_part(owner) || !is_repository_part(repo) {
        return None;
    }
    if !matches!(architecture, "x86_64" | "aarch64") {
        return None;
    }
    Some(UpdateConfig::GithubRelease(GithubUpdateConfig {
        repo: format!("{owner}/{repo}"),
        asset_regex: asset_regex_from_glob(pattern)?,
        architecture: architecture.to_owned(),
        tag_regex: DEFAULT_TAG_REGEX.to_owned(),
        prerelease,
        channel: channel.to_owned(),
    }))
}

pub(crate) fn config_from_app_update_yml(text: &str) -> Option<UpdateConfig> {
    let mut provider = None;
    let mut owner = None;
    let mut repo = None;
    let mut url = None;
    for line in text.lines().filter(|line| !is_ignorable(line)) {
        if line.starts_with([' ', '\t']) {
            continue;
        }
        let Some((key, value)) = key_value(line.trim_end()) else {
            continue;
        };
        let slot = match key {
            "provider" => &mut provider,
            "owner" => &mut owner,
            "repo" => &mut repo,
            "url" => &mut url,
            _ => continue,
        };
        slot.get_or_insert(value);
    }
    match provider.as_deref()? {
        "github" => {
            let (owner, repo) = (owner?, repo?);
            (is_repository_part(&owner) && is_repository_part(&repo)).then(|| {
                UpdateConfig::ElectronFeed {
                    url: format!("https://github.com/{owner}/{repo}/releases/latest/download"),
                }
            })
        }
        "generic" => {
            let url = url?.trim_end_matches('/').to_owned();
            url.starts_with("https://")
                .then_some(UpdateConfig::ElectronFeed { url })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(repo: &str, asset_regex: &str, prerelease: bool) -> UpdateConfig {
        UpdateConfig::GithubRelease(GithubUpdateConfig {
            repo: repo.into(),
            asset_regex: asset_regex.into(),
            architecture: "x86_64".into(),
            tag_regex: DEFAULT_TAG_REGEX.into(),
            prerelease,
            channel: if prerelease { "prerelease" } else { "stable" }.into(),
        })
    }

    fn feed(url: &str) -> Option<UpdateConfig> {
        Some(UpdateConfig::ElectronFeed { url: url.into() })
    }

    #[test]
    fn maps_upd_info_strings() {
        let cases: Vec<(&str, Option<UpdateConfig>)> = vec![
            (
                "gh-releases-zsync|cryptomator|cryptomator|latest|cryptomator-*-x86_64.AppImage.zsync",
                Some(release("cryptomator/cryptomator", r"^cryptomator-.*-x86_64\.AppImage$", false)),
            ),
            (
                "gh-releases-zsync|pch|rawmakase|latest|rawmakase-*-x86_64.AppImage.zsync\n",
                Some(release("pch/rawmakase", r"^rawmakase-.*-x86_64\.AppImage$", false)),
            ),
            (
                "gh-releases-zsync|o|r.js|latest-pre|App+(1)-*.AppImage.zsync",
                Some(release("o/r.js", r"^App\+\(1\)-.*\.AppImage$", true)),
            ),
            (
                "gh-releases-zsync|o|r|latest-all|r-*.AppImage",
                Some(release("o/r", r"^r-.*\.AppImage$", true)),
            ),
            (
                "zsync|https://example.org/App.AppImage.zsync",
                Some(UpdateConfig::Zsync { url: "https://example.org/App.AppImage.zsync".into() }),
            ),
            ("gh-releases-zsync|o|r|v1.2.3|r-*.AppImage.zsync", None),
            ("gh-releases-zsync|o|r|latest", None),
            ("gh-releases-zsync|o|r|latest|r.AppImage.zsync|extra", None),
            ("gh-releases-zsync|o/x|r|latest|r.AppImage.zsync", None),
            ("gh-releases-zsync|..|r|latest|r.AppImage.zsync", None),
            ("gh-releases-zsync|o|r|latest|.zsync", None),
            ("pling-v1-zsync|123|App.AppImage|", None),
            ("zsync|http://example.org/App.AppImage.zsync", None),
            ("zsync|App.AppImage.zsync", None),
            ("guess", None),
            ("", None),
        ];
        for (info, expected) in cases {
            assert_eq!(config_from_upd_info(info, "x86_64"), expected, "{info:?}");
            if let Some(config) = expected {
                assert!(crate::update_sources::validate_config(&config).is_ok());
            }
        }
    }

    #[test]
    fn glob_patterns_become_anchored_regexes_for_asset_names() {
        let regex = regex::Regex::new(
            &asset_regex_from_glob("cryptomator-*-x86_64.AppImage.zsync").unwrap(),
        )
        .unwrap();
        assert!(regex.is_match("cryptomator-1.5.15-x86_64.AppImage"));
        assert!(!regex.is_match("cryptomator-1.5.15-x86_64.AppImage.zsync"));
        assert!(!regex.is_match("cryptomator-1.5.15-x86_64xAppImage"));
        assert!(!regex.is_match("other-cryptomator-1.5.15-x86_64.AppImage"));
    }

    #[test]
    fn upd_info_uses_the_host_architecture() {
        let Some(UpdateConfig::GithubRelease(config)) =
            config_from_upd_info("gh-releases-zsync|o|r|latest|r-*.AppImage.zsync", "aarch64")
        else {
            panic!("expected a GitHub release");
        };
        assert_eq!(config.architecture, "aarch64");
        assert_eq!(
            config_from_upd_info("gh-releases-zsync|o|r|latest|r.AppImage", "riscv64"),
            None
        );
    }

    #[test]
    fn maps_app_update_yml_contents() {
        let cases: Vec<(&str, Option<UpdateConfig>)> = vec![
            (
                "owner: beekeeper-studio\nrepo: beekeeper-studio\nprovider: github\nupdaterCacheDirName: beekeeper-studio-updater\n",
                feed("https://github.com/beekeeper-studio/beekeeper-studio/releases/latest/download"),
            ),
            (
                "provider: generic\nurl: https://download.live.ledger.com\nupdaterCacheDirName: ledger-live-desktop-updater\n",
                feed("https://download.live.ledger.com"),
            ),
            (
                "provider: 'generic'\nurl: \"https://downloads.example/app/\"\n",
                feed("https://downloads.example/app"),
            ),
            (
                "# generated\nowner: o\nrepo: r\nprovider: github\ntagNamePrefix: v\nupdaterCacheDirName: '@spawndesktop-updater'\n",
                feed("https://github.com/o/r/releases/latest/download"),
            ),
            ("provider: generic\nurl: http://download.example\n", None),
            ("provider: generic\n", None),
            ("provider: github\nowner: o\n", None),
            ("provider: github\nowner: ../x\nrepo: r\n", None),
            ("provider: github\nowner: o\nrepo: a b\n", None),
            ("provider: s3\nbucket: b\n", None),
            ("provider: spaces\nname: n\nregion: r\n", None),
            ("owner: o\nrepo: r\n", None),
            ("", None),
        ];
        for (text, expected) in cases {
            assert_eq!(config_from_app_update_yml(text), expected, "{text:?}");
        }
    }

    #[test]
    fn app_update_yml_ignores_nested_keys() {
        let text =
            "provider: generic\nurl: https://top.example\nextra:\n  url: https://nested.example\n";
        assert_eq!(
            config_from_app_update_yml(text),
            feed("https://top.example")
        );
    }

    fn detected_release() -> UpdateConfig {
        release("o/r", "^r$", false)
    }

    fn saved(
        config: Option<UpdateConfig>,
        origin: Option<UpdateConfigOrigin>,
    ) -> Option<UpdateSource> {
        Some(UpdateSource { config, origin })
    }

    #[test]
    fn detects_the_source_of_a_new_app() {
        let source = source_after_discovery(None, false, || Some(detected_release()));
        assert_eq!(source.config, Some(detected_release()));
        assert_eq!(source.origin, Some(UpdateConfigOrigin::Detected));
        let none = source_after_discovery(None, false, || None);
        assert_eq!((none.config, none.origin), (None, None));
    }

    #[test]
    fn backfills_apps_saved_before_detection_existed() {
        let source = source_after_discovery(saved(None, None), false, || Some(detected_release()));
        assert_eq!(source.origin, Some(UpdateConfigOrigin::Detected));
        assert_eq!(source.config, Some(detected_release()));
        let manual = UpdateConfig::DirectUrl {
            url: "https://example.org/a".into(),
        };
        let kept = source_after_discovery(saved(Some(manual.clone()), None), true, || {
            panic!("legacy manual configs are not re-detected")
        });
        assert_eq!(kept.config, Some(manual));
    }

    #[test]
    fn never_overrides_the_users_choice() {
        let cleared =
            source_after_discovery(saved(None, Some(UpdateConfigOrigin::User)), true, || {
                panic!("a cleared source is not detected again")
            });
        assert_eq!(
            (cleared.config, cleared.origin),
            (None, Some(UpdateConfigOrigin::User))
        );
        let manual = UpdateConfig::DirectUrl {
            url: "https://example.org/a".into(),
        };
        let kept = source_after_discovery(
            saved(Some(manual.clone()), Some(UpdateConfigOrigin::User)),
            true,
            || panic!("a user-set source is not detected again"),
        );
        assert_eq!(kept.config, Some(manual));
    }

    #[test]
    fn refreshes_detected_sources_only_when_the_file_changed() {
        let old = UpdateConfig::Zsync {
            url: "https://example.org/old.zsync".into(),
        };
        let existing = || saved(Some(old.clone()), Some(UpdateConfigOrigin::Detected));
        let unchanged = source_after_discovery(existing(), false, || panic!("file is unchanged"));
        assert_eq!(unchanged.config, Some(old.clone()));
        let replaced = source_after_discovery(existing(), true, || Some(detected_release()));
        assert_eq!(replaced.config, Some(detected_release()));
        assert_eq!(replaced.origin, Some(UpdateConfigOrigin::Detected));
        let kept = source_after_discovery(existing(), true, || None);
        assert_eq!(kept.config, Some(old));
        assert_eq!(kept.origin, Some(UpdateConfigOrigin::Detected));
    }

    #[test]
    fn detects_from_the_update_information_section_without_running_the_file() {
        let directory = crate::persistence::tests::TemporaryDirectory::new();
        let path = directory.0.join("App.AppImage");
        let mut image =
            crate::elf_sections::synthetic_elf(Some(b"zsync|https://example.org/App.zsync\0\0"));
        image.resize(image.len() + 64, 0);
        std::fs::write(&path, image).unwrap();
        assert_eq!(
            detect_logged(&path),
            Some(UpdateConfig::Zsync {
                url: "https://example.org/App.zsync".into()
            })
        );
        let plain = directory.0.join("Plain.AppImage");
        std::fs::write(&plain, crate::elf_sections::synthetic_elf(None)).unwrap();
        assert_eq!(detect_logged(&plain), None);
    }
}
