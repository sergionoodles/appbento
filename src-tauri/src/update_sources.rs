//! Resolve updates from the configuration saved with an installed app.
use serde::Deserialize;

use crate::github::{self, Release, ReleaseAsset, ResolvedUpdate};
use crate::models::{GithubUpdateConfig, UpdateConfig};

#[derive(Deserialize)]
struct GitlabRelease {
    tag_name: String,
    released_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    upcoming_release: bool,
    assets: GitlabAssets,
}

#[derive(Deserialize)]
struct GitlabAssets {
    links: Vec<GitlabAsset>,
}

#[derive(Deserialize)]
struct GitlabAsset {
    name: String,
    url: String,
    direct_asset_url: Option<String>,
}

pub fn validate_config(config: &UpdateConfig) -> Result<(), String> {
    match config {
        UpdateConfig::GithubRelease(config) => github::validate_config(config),
        UpdateConfig::GitlabRelease(config) => {
            github::validate_release_config(config)?;
            gitlab_api_url(&config.repo).map(|_| ())
        }
        UpdateConfig::DirectUrl { url } => github::validate_https_url(url).map(|_| ()),
    }
}

pub async fn resolve(config: &UpdateConfig) -> Result<ResolvedUpdate, String> {
    validate_config(config)?;
    match config {
        UpdateConfig::GithubRelease(config) => github::check_github(config).await,
        UpdateConfig::GitlabRelease(config) => {
            let url = gitlab_api_url(&config.repo)?;
            let bytes = github::fetch_release_bytes(&url).await?;
            resolve_gitlab_bytes(config, &bytes)
        }
        UpdateConfig::DirectUrl { url } => resolve_direct_url(url),
    }
}

fn resolve_direct_url(value: &str) -> Result<ResolvedUpdate, String> {
    let url = github::validate_https_url(value)?;
    let asset_name = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.is_empty())
        .unwrap_or("update.AppImage")
        .to_string();
    Ok(ResolvedUpdate {
        latest: String::new(),
        download_url: value.to_string(),
        asset_name,
        published_at: None,
        size_bytes: None,
    })
}

fn gitlab_api_url(value: &str) -> Result<String, String> {
    let mut url = github::validate_https_url(value)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err("repository URL cannot include query parameters or a fragment".into());
    }
    let project = url.path().trim_matches('/').trim_end_matches(".git");
    let parts: Vec<_> = project.split('/').collect();
    if parts.len() < 2
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == "-"
                || !part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
                })
        })
    {
        return Err(
            "enter an HTTPS GitLab repository URL including its namespace and project".into(),
        );
    }
    // GitLab treats the complete namespace/project path as one encoded API segment.
    let encoded_project = project.replace('/', "%2F");
    url.set_path(&format!("/api/v4/projects/{encoded_project}/releases"));
    url.set_query(Some("per_page=100&order_by=released_at&sort=desc"));
    Ok(url.to_string())
}

fn resolve_gitlab_bytes(
    config: &GithubUpdateConfig,
    bytes: &[u8],
) -> Result<ResolvedUpdate, String> {
    let releases: Vec<GitlabRelease> = serde_json::from_slice(bytes)
        .map_err(|error| format!("invalid GitLab release response: {error}"))?;
    let tag_expression = regex::Regex::new(&config.tag_regex).map_err(|error| error.to_string())?;
    let releases = releases
        .into_iter()
        .map(|release| Release {
            // GitLab has no prerelease flag; use the standard suffix in release tags.
            prerelease: tag_expression
                .captures(&release.tag_name)
                .and_then(|captures| captures.name("version"))
                .is_some_and(|version| version.as_str().contains('-')),
            draft: release.upcoming_release,
            tag_name: release.tag_name,
            published_at: release.released_at,
            assets: release
                .assets
                .links
                .into_iter()
                .map(|asset| ReleaseAsset {
                    name: asset.name,
                    browser_download_url: asset
                        .direct_asset_url
                        .filter(|url| !url.is_empty())
                        .unwrap_or(asset.url),
                    size: None,
                })
                .collect(),
        })
        .collect();
    github::select_release(config, releases, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> GithubUpdateConfig {
        GithubUpdateConfig {
            repo: "https://gitlab.example/team/subgroup/app".into(),
            asset_regex: r".*\.AppImage$".into(),
            architecture: std::env::consts::ARCH.into(),
            tag_regex: r"^v?(?P<version>[0-9]+(?:\.[0-9]+)+(?:-[a-z0-9.]+)?)$".into(),
            prerelease: false,
            channel: "stable".into(),
        }
    }

    #[test]
    fn encodes_nested_gitlab_project_paths() {
        assert_eq!(gitlab_api_url("https://gitlab.example/team/subgroup/app.git/").unwrap(),
            "https://gitlab.example/api/v4/projects/team%2Fsubgroup%2Fapp/releases?per_page=100&order_by=released_at&sort=desc");
        assert!(gitlab_api_url("https://gitlab.example/team/app?token=secret").is_err());
        assert!(gitlab_api_url("https://gitlab.example/team/-/releases").is_err());
    }

    #[test]
    fn resolves_gitlab_link_and_preserves_release_metadata() {
        let bytes = br#"[{"tag_name":"v2.0.0","released_at":"2026-01-02T03:04:05Z","assets":{"links":[{"name":"App.AppImage","url":"https://cdn.example/app","direct_asset_url":"https://gitlab.example/team/app/download"}]}}]"#;
        let resolved = resolve_gitlab_bytes(&config(), bytes).unwrap();
        assert_eq!(resolved.latest, "2.0.0");
        assert_eq!(
            resolved.download_url,
            "https://gitlab.example/team/app/download"
        );
        assert!(resolved.published_at.is_some());
        assert_eq!(resolved.size_bytes, None);
    }

    #[test]
    fn filters_gitlab_prereleases_and_upcoming_releases() {
        let bytes = br#"[{"tag_name":"v3.0.0-beta.1","assets":{"links":[{"name":"App.AppImage","url":"https://cdn.example/beta"}]}},{"tag_name":"v4.0.0","upcoming_release":true,"assets":{"links":[{"name":"App.AppImage","url":"https://cdn.example/upcoming"}]}},{"tag_name":"v2.0.0","assets":{"links":[{"name":"App.AppImage","url":"https://cdn.example/stable"}]}}]"#;
        assert_eq!(
            resolve_gitlab_bytes(&config(), bytes).unwrap().latest,
            "2.0.0"
        );
        let mut prerelease_config = config();
        prerelease_config.channel = "prerelease".into();
        assert_eq!(
            resolve_gitlab_bytes(&prerelease_config, bytes)
                .unwrap()
                .latest,
            "3.0.0-beta.1"
        );
    }

    #[test]
    fn rejects_unsafe_and_ambiguous_gitlab_assets() {
        let unsafe_asset = br#"[{"tag_name":"v2.0.0","assets":{"links":[{"name":"App.AppImage","url":"http://cdn.example/app"}]}}]"#;
        assert!(resolve_gitlab_bytes(&config(), unsafe_asset).is_err());
        let ambiguous = br#"[{"tag_name":"v2.0.0","assets":{"links":[{"name":"A.AppImage","url":"https://cdn.example/a"},{"name":"B.AppImage","url":"https://cdn.example/b"}]}}]"#;
        assert!(resolve_gitlab_bytes(&config(), ambiguous).is_err());
    }

    #[test]
    fn resolves_direct_urls_without_inventing_a_version() {
        let resolved =
            resolve_direct_url("https://downloads.example/App.AppImage?download=1").unwrap();
        assert_eq!(resolved.asset_name, "App.AppImage");
        assert!(resolved.latest.is_empty());
        assert_eq!(resolved.size_bytes, None);
        assert_eq!(
            resolve_direct_url("https://example.com/")
                .unwrap()
                .asset_name,
            "update.AppImage"
        );
        for url in [
            "http://example.com/app",
            "https://user:password@example.com/app",
            "file:///app",
        ] {
            assert!(resolve_direct_url(url).is_err());
        }
    }
}
