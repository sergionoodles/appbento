//! Resolve compatible GitHub release assets; downloads remain a manual operation.
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde::Deserialize;

use crate::models::GithubUpdateConfig;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RELEASE_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const RELEASES_PER_PAGE: usize = 10;
const MAX_RELEASE_PAGES: usize = 10;
static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

#[derive(Debug, Deserialize)]
pub(crate) struct ReleaseAsset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
    pub(crate) size: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Release {
    pub(crate) tag_name: String,
    #[serde(default)]
    pub(crate) prerelease: bool,
    #[serde(default)]
    pub(crate) draft: bool,
    pub(crate) published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) assets: Vec<ReleaseAsset>,
}

pub struct ResolvedUpdate {
    pub latest: String,
    pub download_url: String,
    pub asset_name: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub size_bytes: Option<u64>,
}

pub fn validate_config(config: &GithubUpdateConfig) -> Result<(), String> {
    let repository =
        Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$").map_err(|error| error.to_string())?;
    if !repository.is_match(&config.repo) {
        return Err("invalid GitHub owner/repository".into());
    }
    validate_release_config(config)?;
    Ok(())
}

pub(crate) fn validate_release_config(config: &GithubUpdateConfig) -> Result<(), String> {
    Regex::new(&config.asset_regex).map_err(|error| format!("bad asset_regex: {error}"))?;
    let tag = Regex::new(&config.tag_regex).map_err(|error| format!("bad tag_regex: {error}"))?;
    if !tag.capture_names().any(|name| name == Some("version")) {
        return Err("tag_regex requires a named version capture".into());
    }
    if !matches!(config.architecture.as_str(), "x86_64" | "aarch64") {
        return Err("unsupported architecture".into());
    }
    if !matches!(config.channel.as_str(), "stable" | "prerelease") {
        return Err("channel must be stable or prerelease".into());
    }
    Ok(())
}

pub(crate) fn validate_https_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|error| error.to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("update URLs must use HTTPS without credentials".into());
    }
    Ok(url)
}

pub(crate) fn client() -> Result<&'static reqwest::Client, String> {
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent("appbento/0.1")
                .connect_timeout(CONNECTION_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .https_only(true)
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() >= 10 {
                        return attempt.error("too many redirects");
                    }
                    if validate_https_url(attempt.url().as_str()).is_err() {
                        return attempt.error("unsafe update redirect");
                    }
                    attempt.follow()
                }))
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) async fn fetch_release_bytes(url: &str) -> Result<Vec<u8>, String> {
    validate_https_url(url)?;
    let mut response = client()?
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RELEASE_RESPONSE_BYTES {
            return Err("release response exceeds size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn check_github(config: &GithubUpdateConfig) -> Result<ResolvedUpdate, String> {
    check_github_with_fetch(config, |url| async move { fetch_release_bytes(&url).await }).await
}

async fn check_github_with_fetch<F, Fetch>(
    config: &GithubUpdateConfig,
    mut fetch: F,
) -> Result<ResolvedUpdate, String>
where
    F: FnMut(String) -> Fetch,
    Fetch: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    validate_config(config)?;
    for page in 1..=MAX_RELEASE_PAGES {
        let url = format!(
            "https://api.github.com/repos/{}/releases?per_page={RELEASES_PER_PAGE}&page={page}",
            config.repo
        );
        let bytes = fetch(url).await?;
        let releases: Vec<Release> =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let is_last_page = releases.len() < RELEASES_PER_PAGE;
        if let Some(update) = find_matching_release(config, releases, true)? {
            return Ok(update);
        }
        if is_last_page {
            break;
        }
    }
    Err(format!(
        "no matching compatible AppImage asset for {}",
        config.repo
    ))
}

fn asset_matches_architecture(name: &str, architecture: &str) -> bool {
    let normalized = name.to_ascii_lowercase().replace("x86_64", "amd64");
    let tokens: Vec<_> = normalized.split(['-', '_', '.']).collect();
    let has_arm64 = tokens
        .iter()
        .any(|token| matches!(*token, "arm64" | "aarch64"));
    let has_x86_64 = tokens.iter().any(|token| matches!(*token, "amd64" | "x64"));
    let has_incompatible = tokens.iter().any(|token| {
        matches!(*token, "arm" | "armhf" | "x86")
            || token.strip_prefix("armv").is_some_and(|suffix| {
                suffix.starts_with(|character: char| character.is_ascii_digit())
            })
            || ["riscv", "ppc", "powerpc", "s390", "loongarch"]
                .iter()
                .any(|prefix| token.starts_with(prefix))
            || (token.len() == 4
                && token.starts_with('i')
                && token.ends_with("86")
                && matches!(token.as_bytes()[1], b'3'..=b'6'))
    });
    if has_incompatible || (has_arm64 && has_x86_64) {
        return false;
    }
    // Unlabelled assets inherit the architecture selected in the saved configuration.
    match architecture {
        "x86_64" => !has_arm64,
        "aarch64" => !has_x86_64,
        _ => false,
    }
}

pub(crate) fn select_release(
    config: &GithubUpdateConfig,
    releases: Vec<Release>,
    restrict_github_url: bool,
) -> Result<ResolvedUpdate, String> {
    find_matching_release(config, releases, restrict_github_url)?
        .ok_or_else(|| format!("no matching compatible AppImage asset for {}", config.repo))
}

fn find_matching_release(
    config: &GithubUpdateConfig,
    releases: Vec<Release>,
    restrict_github_url: bool,
) -> Result<Option<ResolvedUpdate>, String> {
    if config.architecture != std::env::consts::ARCH {
        return Err(format!(
            "configured assets do not support {}",
            std::env::consts::ARCH
        ));
    }
    let expression = Regex::new(&config.asset_regex).map_err(|error| error.to_string())?;
    let tag_expression = Regex::new(&config.tag_regex).map_err(|error| error.to_string())?;
    for release in releases {
        if release.draft
            || (release.prerelease && !config.prerelease && config.channel != "prerelease")
        {
            continue;
        }
        let Some(captures) = tag_expression.captures(&release.tag_name) else {
            continue;
        };
        let Some(version) = captures
            .name("version")
            .filter(|version| !version.as_str().is_empty())
        else {
            continue;
        };
        let assets: Vec<_> = release
            .assets
            .iter()
            .filter(|asset| {
                asset.name.to_ascii_lowercase().ends_with(".appimage")
                    && expression.is_match(&asset.name)
                    && asset_matches_architecture(&asset.name, &config.architecture)
            })
            .collect();
        if assets.len() > 1 {
            return Err(format!(
                "ambiguous compatible AppImage assets for {}",
                config.repo
            ));
        }
        let Some(asset) = assets.first() else {
            continue;
        };
        let url = validate_https_url(&asset.browser_download_url)?;
        if restrict_github_url
            && (url.host_str() != Some("github.com")
                || !url
                    .path()
                    .starts_with(&format!("/{}/releases/download/", config.repo)))
        {
            return Err("release asset URL is outside its trusted GitHub repository".into());
        }
        let latest = version.as_str().to_string();
        return Ok(Some(ResolvedUpdate {
            latest,
            download_url: asset.browser_download_url.clone(),
            asset_name: asset.name.clone(),
            published_at: release.published_at,
            size_bytes: asset.size,
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> GithubUpdateConfig {
        GithubUpdateConfig {
            repo: "owner/app".into(),
            asset_regex: r".*\.AppImage$".into(),
            architecture: "x86_64".into(),
            tag_regex: r"^v?(?P<version>[0-9]+(?:\.[0-9]+)+)$".into(),
            prerelease: false,
            channel: "stable".into(),
        }
    }

    fn release(names: &[&str]) -> Release {
        Release {
            tag_name: "v2.0.0".into(),
            prerelease: false,
            draft: false,
            published_at: None,
            assets: names
                .iter()
                .map(|name| ReleaseAsset {
                    name: (*name).into(),
                    browser_download_url: format!(
                        "https://github.com/owner/app/releases/download/v2.0.0/{name}"
                    ),
                    size: Some(100),
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn checks_small_pages_until_a_compatible_release_is_found() {
        let mut requested_urls = Vec::new();
        let resolved = check_github_with_fetch(&config(), |url| {
            requested_urls.push(url);
            let releases: Vec<_> = if requested_urls.len() == 1 {
                (0..10).map(|_| serde_json::json!({
                    "tag_name": "nightly", "assets": [],
                    "body": "x".repeat(MAX_RELEASE_RESPONSE_BYTES / 100),
                })).collect()
            } else {
                vec![serde_json::json!({
                    "tag_name": "v2.0.0",
                    "assets": [{
                        "name": "app-x86_64.AppImage",
                        "browser_download_url": "https://github.com/owner/app/releases/download/v2.0.0/app-x86_64.AppImage",
                    }],
                })]
            };
            let bytes = serde_json::to_vec(&releases).unwrap();
            assert!(bytes.len() < MAX_RELEASE_RESPONSE_BYTES);
            async move { Ok(bytes) }
        }).await.unwrap();
        assert_eq!(resolved.latest, "2.0.0");
        assert_eq!(
            requested_urls,
            vec![
                "https://api.github.com/repos/owner/app/releases?per_page=10&page=1",
                "https://api.github.com/repos/owner/app/releases?per_page=10&page=2",
            ]
        );
    }

    #[tokio::test]
    async fn stops_at_the_first_match_or_the_last_page() {
        let matching = serde_json::json!([{
            "tag_name": "v2.0.0",
            "assets": [{
                "name": "app-x86_64.AppImage",
                "browser_download_url": "https://github.com/owner/app/releases/download/v2.0.0/app-x86_64.AppImage",
            }],
        }]);
        let mut requests = 0;
        let resolved = check_github_with_fetch(&config(), |_| {
            requests += 1;
            let bytes = serde_json::to_vec(&matching).unwrap();
            async move { Ok(bytes) }
        })
        .await
        .unwrap();
        assert_eq!(resolved.latest, "2.0.0");
        assert_eq!(requests, 1);

        let mut requests = 0;
        let error = check_github_with_fetch(&config(), |_| {
            requests += 1;
            async { Ok(b"[]".to_vec()) }
        })
        .await
        .err()
        .unwrap();
        assert_eq!(error, "no matching compatible AppImage asset for owner/app");
        assert_eq!(requests, 1);
    }

    #[tokio::test]
    async fn bounds_the_release_search_and_propagates_fetch_errors() {
        let mut requests = 0;
        let error = check_github_with_fetch(&config(), |_| {
            requests += 1;
            let bytes = serde_json::to_vec(&vec![
                serde_json::json!({"tag_name": "nightly", "assets": []});
                10
            ])
            .unwrap();
            async move { Ok(bytes) }
        })
        .await
        .err()
        .unwrap();
        assert_eq!(error, "no matching compatible AppImage asset for owner/app");
        assert_eq!(requests, 10);

        let error = check_github_with_fetch(&config(), |_| async {
            Err("release response exceeds size limit".into())
        })
        .await
        .err()
        .unwrap();
        assert_eq!(error, "release response exceeds size limit");
    }

    #[test]
    fn skips_unmatched_tags_and_draft_releases() {
        let mut unmatched = release(&["app-x86_64.AppImage"]);
        unmatched.tag_name = "nightly".into();
        let mut draft = release(&["app-x86_64.AppImage"]);
        draft.draft = true;
        draft.tag_name = "v3.0.0".into();
        let resolved = select_release(
            &config(),
            vec![unmatched, draft, release(&["app-x86_64.AppImage"])],
            true,
        )
        .unwrap();
        assert_eq!(resolved.latest, "2.0.0");
    }

    #[test]
    fn includes_prereleases_only_when_enabled() {
        let mut prerelease = release(&["app-x86_64.AppImage"]);
        prerelease.prerelease = true;
        assert!(select_release(&config(), vec![prerelease], true).is_err());
        let mut config = config();
        config.prerelease = true;
        let mut prerelease = release(&["app-x86_64.AppImage"]);
        prerelease.prerelease = true;
        assert!(select_release(&config, vec![prerelease], true).is_ok());
    }

    #[test]
    fn rejects_non_appimage_assets_even_when_pattern_matches() {
        let mut config = config();
        config.asset_regex = ".*".into();
        assert!(select_release(&config, vec![release(&["app-x86_64.zip"])], true).is_err());
    }

    #[test]
    fn resolves_host_asset_instead_of_first_asset() {
        let resolved = select_release(
            &config(),
            vec![release(&["app-arm64.AppImage", "app-x86_64.AppImage"])],
            true,
        )
        .unwrap();
        assert_eq!(resolved.asset_name, "app-x86_64.AppImage");
        assert_eq!(resolved.latest, "2.0.0");
    }

    #[test]
    fn rejects_ambiguous_assets_and_untrusted_urls() {
        assert!(select_release(
            &config(),
            vec![release(&["app-x64.AppImage", "app-x86_64.AppImage"])],
            true
        )
        .is_err());
        let mut malicious = release(&["app-x86_64.AppImage"]);
        malicious.assets[0].browser_download_url = "https://evil.example/app".into();
        assert!(select_release(&config(), vec![malicious], true).is_err());
    }

    #[test]
    fn rejects_delimited_incompatible_architecture_tokens() {
        for filename in [
            "app-arm.AppImage",
            "app-armv8.AppImage",
            "app-i586.AppImage",
            "app-riscv64.AppImage",
            "app-ppc64le.AppImage",
            "app-s390x.AppImage",
        ] {
            assert!(
                !asset_matches_architecture(filename, "x86_64"),
                "{filename}"
            );
        }
        assert!(asset_matches_architecture("farm-1.0.AppImage", "x86_64"));
        assert!(asset_matches_architecture("app-x86_64.AppImage", "x86_64"));
    }

    #[test]
    fn rejects_other_architectures_and_invalid_configuration() {
        assert!(select_release(&config(), vec![release(&["app-arm64.AppImage"])], true).is_err());
        assert!(!asset_matches_architecture("app-i686.AppImage", "x86_64"));
        let mut invalid = config();
        invalid.repo = "owner/app?redirect=evil".into();
        assert!(validate_config(&invalid).is_err());
    }
}
