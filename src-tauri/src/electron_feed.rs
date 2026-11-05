//! Resolve updates from an electron-builder feed (`latest-linux.yml`) without downloading the app.
use crate::github::{self, ExpectedChecksum, ResolvedUpdate};
use crate::simple_yaml::{is_ignorable, key_value};

const MAX_FEED_BYTES: usize = 64 * 1024;
const APPIMAGE_SUFFIX: &str = ".appimage";

#[derive(Debug, Default, PartialEq, Eq)]
struct FeedFile {
    url: String,
    sha512: Option<String>,
    size: Option<u64>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Feed {
    version: String,
    path: Option<String>,
    sha512: Option<String>,
    release_date: Option<String>,
    files: Vec<FeedFile>,
}

pub(crate) fn feed_file_name(architecture: &str) -> &'static str {
    if architecture == "aarch64" {
        "latest-linux-arm64.yml"
    } else {
        "latest-linux.yml"
    }
}

/// Appends a file name to the feed directory URL, keeping any existing path.
fn child_url(base: &str, file_name: &str) -> Result<reqwest::Url, String> {
    let mut url = github::validate_https_url(base)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err("feed URL cannot include query parameters or a fragment".into());
    }
    let path = format!("{}/{file_name}", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

fn apply_file_field(file: &mut FeedFile, key: &str, value: String) {
    match key {
        "url" => file.url = value,
        "sha512" if !value.is_empty() => file.sha512 = Some(value),
        "size" => file.size = value.parse().ok(),
        _ => {}
    }
}

fn parse_feed(text: &str) -> Result<Feed, String> {
    let mut feed = Feed::default();
    let mut in_files = false;
    for line in text.lines().filter(|line| !is_ignorable(line)) {
        let trimmed = line.trim();
        let is_indented = line.starts_with([' ', '\t']);
        let item = trimmed
            .strip_prefix("- ")
            .or_else(|| (trimmed == "-").then_some(""));
        if let (true, Some(rest)) = (in_files, item) {
            feed.files.push(FeedFile::default());
            if let (Some((key, value)), Some(file)) =
                (key_value(rest.trim()), feed.files.last_mut())
            {
                apply_file_field(file, key, value);
            }
            continue;
        }
        let Some((key, value)) = key_value(trimmed) else {
            continue;
        };
        if is_indented {
            if let (true, Some(file)) = (in_files, feed.files.last_mut()) {
                apply_file_field(file, key, value);
            }
            continue;
        }
        in_files = key == "files" && value.is_empty();
        match key {
            "version" => feed.version = value,
            "path" if !value.is_empty() => feed.path = Some(value),
            "sha512" if !value.is_empty() => feed.sha512 = Some(value),
            "releaseDate" => feed.release_date = Some(value),
            _ => {}
        }
    }
    if feed.version.is_empty() {
        return Err("update feed has no version".into());
    }
    Ok(feed)
}

fn is_appimage(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(APPIMAGE_SUFFIX)
}

fn select_file(feed: &Feed) -> Result<FeedFile, String> {
    let appimages: Vec<_> = feed
        .files
        .iter()
        .filter(|file| is_appimage(&file.url))
        .collect();
    match appimages.as_slice() {
        [file] => Ok(FeedFile {
            url: file.url.clone(),
            sha512: file.sha512.clone(),
            size: file.size,
        }),
        [] if feed.files.is_empty() => feed
            .path
            .as_ref()
            .filter(|path| is_appimage(path))
            .map(|path| FeedFile {
                url: path.clone(),
                sha512: feed.sha512.clone(),
                size: None,
            })
            .ok_or_else(|| "update feed lists no AppImage".into()),
        [] => Err("update feed lists no AppImage".into()),
        _ => Err("update feed lists several AppImages (ambiguous)".into()),
    }
}

fn resolve_feed(base_url: &str, text: &str) -> Result<ResolvedUpdate, String> {
    let feed = parse_feed(text)?;
    let file = select_file(&feed)?;
    let mut base = github::validate_https_url(base_url)?;
    let directory = format!("{}/", base.path().trim_end_matches('/'));
    base.set_path(&directory);
    let download = base
        .join(&file.url)
        .map_err(|error| format!("invalid update feed URL: {error}"))?;
    github::validate_https_url(download.as_str())?;
    let asset_name = download
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.is_empty())
        .ok_or("update feed file has no name")?
        .to_owned();
    Ok(ResolvedUpdate {
        latest: feed.version,
        download_url: download.to_string(),
        asset_name,
        published_at: feed
            .release_date
            .and_then(|date| chrono::DateTime::parse_from_rfc3339(&date).ok())
            .map(|date| date.with_timezone(&chrono::Utc)),
        size_bytes: file.size,
        checksum: file.sha512.map(ExpectedChecksum::Sha512Base64),
    })
}

pub(crate) async fn resolve(base_url: &str) -> Result<ResolvedUpdate, String> {
    resolve_with_fetch(base_url, std::env::consts::ARCH, |url| async move {
        github::fetch_limited_bytes(&url, MAX_FEED_BYTES, "text/yaml, */*").await
    })
    .await
}

async fn resolve_with_fetch<F, Fetch>(
    base_url: &str,
    architecture: &str,
    mut fetch: F,
) -> Result<ResolvedUpdate, String>
where
    F: FnMut(String) -> Fetch,
    Fetch: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    let feed_url = child_url(base_url, feed_file_name(architecture))?;
    let bytes = fetch(feed_url.to_string()).await?;
    let text = String::from_utf8(bytes).map_err(|_| "update feed is not valid UTF-8")?;
    resolve_feed(base_url, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = "version: 6.1.4\nfiles:\n  - url: Beekeeper-Studio-6.1.4.AppImage\n    sha512: c2hhNTEy\n    size: 123456\n    blockMapSize: 1234\npath: Beekeeper-Studio-6.1.4.AppImage\nsha512: dG9wLWxldmVs\nreleaseDate: '2025-01-02T03:04:05.000Z'\n";

    #[test]
    fn parses_a_feed_with_a_file_list() {
        let feed = parse_feed(FEED).unwrap();
        assert_eq!(feed.version, "6.1.4");
        assert_eq!(
            feed.path.as_deref(),
            Some("Beekeeper-Studio-6.1.4.AppImage")
        );
        assert_eq!(feed.sha512.as_deref(), Some("dG9wLWxldmVs"));
        assert_eq!(
            feed.files,
            vec![FeedFile {
                url: "Beekeeper-Studio-6.1.4.AppImage".into(),
                sha512: Some("c2hhNTEy".into()),
                size: Some(123456),
            }]
        );
    }

    #[test]
    fn resolves_relative_urls_with_checksum_size_and_date() {
        let resolved = resolve_feed("https://download.example/app/", FEED).unwrap();
        assert_eq!(resolved.latest, "6.1.4");
        assert_eq!(
            resolved.download_url,
            "https://download.example/app/Beekeeper-Studio-6.1.4.AppImage"
        );
        assert_eq!(resolved.asset_name, "Beekeeper-Studio-6.1.4.AppImage");
        assert_eq!(resolved.size_bytes, Some(123456));
        assert_eq!(
            resolved.checksum,
            Some(ExpectedChecksum::Sha512Base64("c2hhNTEy".into()))
        );
        assert_eq!(
            resolved.published_at.map(|date| date.to_rfc3339()),
            Some("2025-01-02T03:04:05+00:00".into())
        );
        let without_slash = resolve_feed("https://download.example/app", FEED).unwrap();
        assert_eq!(without_slash.download_url, resolved.download_url);
    }

    #[test]
    fn accepts_absolute_https_urls_and_rejects_other_schemes() {
        let absolute = FEED.replace(
            "- url: Beekeeper-Studio-6.1.4.AppImage",
            "- url: https://cdn.example/x/App.AppImage",
        );
        assert_eq!(
            resolve_feed("https://download.example", &absolute)
                .unwrap()
                .download_url,
            "https://cdn.example/x/App.AppImage"
        );
        let insecure = FEED.replace(
            "- url: Beekeeper-Studio-6.1.4.AppImage",
            "- url: http://cdn.example/App.AppImage",
        );
        assert!(resolve_feed("https://download.example", &insecure).is_err());
    }

    #[test]
    fn selects_the_single_appimage_among_other_files() {
        let feed = "version: 1.2.3\nfiles:\n  - url: App-1.2.3.deb\n    sha512: a\n  - url: App-1.2.3.APPIMAGE\n    sha512: b\n    size: 9\n  - url: App-1.2.3.rpm\n    sha512: c\n";
        let resolved = resolve_feed("https://download.example", feed).unwrap();
        assert_eq!(resolved.asset_name, "App-1.2.3.APPIMAGE");
        assert_eq!(
            resolved.checksum,
            Some(ExpectedChecksum::Sha512Base64("b".into()))
        );
    }

    #[test]
    fn rejects_ambiguous_and_missing_appimages() {
        let ambiguous =
            "version: 1.2.3\nfiles:\n  - url: A-x64.AppImage\n  - url: A-arm64.AppImage\n";
        assert!(resolve_feed("https://download.example", ambiguous)
            .unwrap_err()
            .contains("ambiguous"));
        let none = "version: 1.2.3\nfiles:\n  - url: A.deb\npath: A.AppImage\n";
        assert!(resolve_feed("https://download.example", none).is_err());
        assert!(resolve_feed("https://download.example", "files:\n  - url: A.AppImage\n").is_err());
    }

    #[test]
    fn falls_back_to_the_top_level_path_without_a_file_list() {
        let feed = "version: 1.2.3\npath: App-1.2.3.AppImage\nsha512: dG9w\n";
        let resolved = resolve_feed("https://download.example", feed).unwrap();
        assert_eq!(resolved.asset_name, "App-1.2.3.AppImage");
        assert_eq!(
            resolved.checksum,
            Some(ExpectedChecksum::Sha512Base64("dG9w".into()))
        );
        assert_eq!(resolved.size_bytes, None);
        assert!(resolve_feed(
            "https://download.example",
            "version: 1.2.3\npath: App.zip\n"
        )
        .is_err());
        assert!(resolve_feed("https://download.example", "version: 1.2.3\n").is_err());
    }

    #[test]
    fn supports_list_items_aligned_with_their_key() {
        let feed = "version: 1.2.3\nfiles:\n- url: App.AppImage\n  size: 5\n";
        let resolved = resolve_feed("https://download.example", feed).unwrap();
        assert_eq!(resolved.size_bytes, Some(5));
    }

    #[test]
    fn rejects_unsafe_feed_base_urls() {
        for base in [
            "http://download.example",
            "https://user:pass@download.example",
            "https://download.example/?token=1",
        ] {
            assert!(child_url(base, "latest-linux.yml").is_err());
        }
    }

    #[tokio::test]
    async fn requests_the_architecture_specific_feed() {
        for (architecture, expected) in [
            ("x86_64", "https://download.example/app/latest-linux.yml"),
            (
                "aarch64",
                "https://download.example/app/latest-linux-arm64.yml",
            ),
        ] {
            let mut requested = Vec::new();
            let resolved =
                resolve_with_fetch("https://download.example/app", architecture, |url| {
                    requested.push(url);
                    async { Ok(FEED.as_bytes().to_vec()) }
                })
                .await
                .unwrap();
            assert_eq!(requested, vec![expected]);
            assert_eq!(resolved.latest, "6.1.4");
        }
    }
}
