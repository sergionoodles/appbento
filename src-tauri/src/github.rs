//! Resolve compatible GitHub release assets; downloads remain a manual operation.
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde::Deserialize;

use crate::models::GithubUpdateConfig;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RELEASE_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    prerelease: bool,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
    assets: Vec<ReleaseAsset>,
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
    Regex::new(&config.asset_regex).map_err(|error| format!("bad asset_regex: {error}"))?;
    let tag = Regex::new(&config.tag_regex).map_err(|error| format!("bad tag_regex: {error}"))?;
    if !tag.capture_names().any(|name| name == Some("version")) {
        return Err("tag_regex requires a named version capture".into());
    }
    if !matches!(config.architecture.as_str(), "x86_64" | "aarch64") {
        return Err("unsupported catalog architecture".into());
    }
    if config.channel != "stable" || config.prerelease {
        return Err("only stable non-prerelease updates are supported".into());
    }
    Ok(())
}

fn client() -> Result<&'static reqwest::Client, String> {
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent("appbento/0.1")
                .connect_timeout(CONNECTION_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .https_only(true)
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub async fn check_github(config: &GithubUpdateConfig) -> Result<ResolvedUpdate, String> {
    validate_config(config)?;
    if config.architecture != std::env::consts::ARCH {
        return Err(format!(
            "catalog assets do not support {}",
            std::env::consts::ARCH
        ));
    }
    let url = format!("https://api.github.com/repos/{}/releases", config.repo);
    let mut response = client()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RELEASE_RESPONSE_BYTES {
            return Err("GitHub release response exceeds size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let releases: Vec<Release> =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    select_release(config, releases)
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
    // Unlabelled assets inherit the explicitly reviewed catalog architecture.
    match architecture {
        "x86_64" => !has_arm64,
        "aarch64" => !has_x86_64,
        _ => false,
    }
}

fn select_release(
    config: &GithubUpdateConfig,
    releases: Vec<Release>,
) -> Result<ResolvedUpdate, String> {
    let expression = Regex::new(&config.asset_regex).map_err(|error| error.to_string())?;
    let tag_expression = Regex::new(&config.tag_regex).map_err(|error| error.to_string())?;
    for release in releases {
        if release.prerelease {
            continue;
        }
        let assets: Vec<_> = release
            .assets
            .iter()
            .filter(|asset| {
                expression.is_match(&asset.name)
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
        let url =
            reqwest::Url::parse(&asset.browser_download_url).map_err(|error| error.to_string())?;
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || !url
                .path()
                .starts_with(&format!("/{}/releases/download/", config.repo))
        {
            return Err("release asset URL is outside its trusted GitHub repository".into());
        }
        let captures = tag_expression
            .captures(&release.tag_name)
            .ok_or("release tag does not match configured version format")?;
        let latest = captures
            .name("version")
            .ok_or("release tag has no version")?
            .as_str()
            .to_string();
        return Ok(ResolvedUpdate {
            latest,
            download_url: asset.browser_download_url.clone(),
            asset_name: asset.name.clone(),
            published_at: release.published_at,
            size_bytes: asset.size,
        });
    }
    Err(format!(
        "no matching compatible AppImage asset for {}",
        config.repo
    ))
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

    #[test]
    fn resolves_host_asset_instead_of_first_asset() {
        let resolved = select_release(
            &config(),
            vec![release(&["app-arm64.AppImage", "app-x86_64.AppImage"])],
        )
        .unwrap();
        assert_eq!(resolved.asset_name, "app-x86_64.AppImage");
        assert_eq!(resolved.latest, "2.0.0");
    }

    #[test]
    fn rejects_ambiguous_assets_and_untrusted_urls() {
        assert!(select_release(
            &config(),
            vec![release(&["app-x64.AppImage", "app-x86_64.AppImage"])]
        )
        .is_err());
        let mut malicious = release(&["app-x86_64.AppImage"]);
        malicious.assets[0].browser_download_url = "https://evil.example/app".into();
        assert!(select_release(&config(), vec![malicious]).is_err());
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
        assert!(select_release(&config(), vec![release(&["app-arm64.AppImage"])]).is_err());
        assert!(!asset_matches_architecture("app-i686.AppImage", "x86_64"));
        let mut invalid = config();
        invalid.repo = "owner/app?redirect=evil".into();
        assert!(validate_config(&invalid).is_err());
    }
}
