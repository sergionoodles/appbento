//! Acquire bounded icon files and keep their contents in the application data directory.
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use reqwest::{Client, Url};
use sha2::{Digest, Sha256};

use crate::persistence;

const NONBLOCKING: i32 = 0x800;
const IMAGE_LIMIT: usize = 5 * 1024 * 1024;
const PAGE_LIMIT: usize = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const FAVICON_CANDIDATE_LIMIT: usize = 8;
const BASE64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("AppBento/0.1")
        .build()
        .map_err(|error| error.to_string())
}

fn parse_url(source: &str) -> Result<Url, String> {
    let url = Url::parse(source).map_err(|error| error.to_string())?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("image and website URLs must use HTTP or HTTPS without credentials".into());
    }
    Ok(url)
}

async fn download(client: &Client, url: Url, limit: usize) -> Result<(Url, Vec<u8>), String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err("download exceeds the size limit".into());
    }
    let final_url = response.url().clone();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if bytes.len() + chunk.len() > limit {
            return Err("download exceeds the size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((final_url, bytes))
}

pub async fn acquire(client: &Client, source: &str) -> Result<Vec<u8>, String> {
    let bytes = if source.starts_with("http://") || source.starts_with("https://") {
        download(client, parse_url(source)?, IMAGE_LIMIT).await?.1
    } else {
        let path = Path::new(source);
        if !path.is_absolute() {
            return Err("select an absolute local file path or an HTTP image URL".into());
        }
        read_image(path)?
    };
    image_format(&bytes)?;
    Ok(bytes)
}

fn read_image(path: &Path) -> Result<Vec<u8>, String> {
    if !fs::metadata(path)
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("icon must be a regular file".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(NONBLOCKING)
        .open(path)
        .map_err(|error| format!("cannot open icon: {error}"))?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("icon must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take((IMAGE_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > IMAGE_LIMIT {
        return Err("icon exceeds the 5 MiB size limit".into());
    }
    image_format(&bytes)?;
    Ok(bytes)
}

pub async fn discover(client: &Client, website: &str) -> Result<Vec<u8>, String> {
    discover_website(client, website).await
}

async fn discover_website(client: &Client, website: &str) -> Result<Vec<u8>, String> {
    let website = parse_url(website)?;
    let page = download(client, website.clone(), PAGE_LIMIT).await;
    let (base, candidates, page_error) = match page {
        Ok((base, bytes)) => {
            let html = String::from_utf8_lossy(&bytes);
            let candidates = favicon_urls(&base, &html)?;
            (base, candidates, None)
        }
        Err(error) => (website, Vec::new(), Some(error)),
    };
    let fallback = base
        .join("/favicon.ico")
        .map_err(|error| error.to_string())?;
    let candidates = candidates.into_iter().chain(std::iter::once(fallback));
    let mut errors = page_error.into_iter().collect::<Vec<_>>();
    for url in candidates {
        match acquire(client, url.as_str()).await {
            Ok(bytes) => return Ok(bytes),
            Err(error) => errors.push(error),
        }
    }
    Err(errors.join("; "))
}

fn favicon_urls(base: &Url, html: &str) -> Result<Vec<Url>, String> {
    let links = Regex::new(r"(?is)<link\b[^>]*>").map_err(|error| error.to_string())?;
    let attributes = Regex::new(r#"(?is)([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#)
        .map_err(|error| error.to_string())?;
    let mut urls = Vec::new();
    for link in links.find_iter(html) {
        let mut relation = String::new();
        let mut href = String::new();
        for attribute in attributes.captures_iter(link.as_str()) {
            let value = attribute
                .get(2)
                .or_else(|| attribute.get(3))
                .or_else(|| attribute.get(4))
                .map(|value| value.as_str())
                .unwrap_or_default();
            match attribute[1].to_ascii_lowercase().as_str() {
                "rel" => relation = value.to_ascii_lowercase(),
                "href" => href = value.replace("&amp;", "&"),
                _ => {}
            }
        }
        if !relation.split_whitespace().any(|value| {
            matches!(
                value,
                "icon" | "apple-touch-icon" | "apple-touch-icon-precomposed"
            )
        }) {
            continue;
        }
        if let Ok(url) = base.join(&href) {
            if !href.is_empty() && parse_url(url.as_str()).is_ok() && !urls.contains(&url) {
                urls.push(url);
            }
        }
        if urls.len() == FAVICON_CANDIDATE_LIMIT {
            break;
        }
    }
    Ok(urls)
}

pub(crate) fn image_format(bytes: &[u8]) -> Result<(&'static str, &'static str), String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        return Ok(("png", "image/png"));
    }
    if bytes.starts_with(b"\xff\xd8\xff") && bytes.len() >= 4 {
        return Ok(("jpg", "image/jpeg"));
    }
    if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 13 {
        return Ok(("gif", "image/gif"));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") && bytes.len() >= 16 {
        return Ok(("webp", "image/webp"));
    }
    if bytes.starts_with(b"\0\0\x01\0") && bytes.len() >= 22 {
        return Ok(("ico", "image/x-icon"));
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        if is_standalone_svg(text)? {
            return Ok(("svg", "image/svg+xml"));
        }
    }
    Err(
        "unsupported image; use PNG, JPEG, GIF, WebP, ICO or a standalone SVG (maximum 5 MiB)"
            .into(),
    )
}

fn is_standalone_svg(text: &str) -> Result<bool, String> {
    // SVG is exposed only through an img data URL. Comments and namespace declarations
    // describe the document; resource loads and executable content remain unsupported.
    let comments = Regex::new(r"(?s)<!--.*?-->").map_err(|error| error.to_string())?;
    let text = comments.replace_all(text, "");
    let declaration =
        Regex::new(r#"(?i)^\s*<\?xml\s+[^?]*\?>"#).map_err(|error| error.to_string())?;
    let text = declaration.replace(text.trim_start_matches('\u{feff}').trim_start(), "");
    let root = Regex::new(r"(?i)^\s*<svg(?:\s|/?>)").map_err(|error| error.to_string())?;
    if !root.is_match(&text) {
        return Ok(false);
    }
    let namespaces = Regex::new(r#"(?i)\sxmlns(?::[\w.-]+)?\s*=\s*(?:"[^"]*"|'[^']*')"#)
        .map_err(|error| error.to_string())?;
    let text = namespaces.replace_all(&text, "");
    let active_tags =
        Regex::new(r"(?i)<\s*(?:[\w.-]+:)?(?:script|foreignobject|iframe|image|use)\b")
            .map_err(|error| error.to_string())?;
    let active_attributes = Regex::new(r"(?i)\s(?:on[\w-]+|(?:[\w.-]+:)?href)\s*=")
        .map_err(|error| error.to_string())?;
    let imports_or_urls =
        Regex::new(r"(?i)@\s*import\b|url\s*\(").map_err(|error| error.to_string())?;
    let lower = text.to_ascii_lowercase();
    let forbidden = [
        "<!",
        "<?",
        "javascript:",
        "data:",
        "http:",
        "https:",
        "file:",
        "ftp:",
        "&#",
        "\\",
    ];
    Ok(!active_tags.is_match(&text)
        && !active_attributes.is_match(&text)
        && !imports_or_urls.is_match(&text)
        && !forbidden.iter().any(|value| lower.contains(value)))
}

pub fn persist_icon(
    library_path: &Path,
    directory: &Path,
    id: &str,
    bytes: &[u8],
    replace: bool,
) -> Result<crate::models::InstalledApp, String> {
    persistence::with_repository(
        library_path,
        |repository: &mut persistence::Repository<crate::store::Library>| {
            crate::lifecycle::recovery::recover(repository)?;
            let app = repository.state.get_mut(id).ok_or("app not found")?;
            if !replace && app.icon_path.is_some() {
                return Ok(app.clone());
            }
            let path = save(directory, bytes)?;
            app.icon_path = Some(path.to_str().ok_or("icon path must be UTF-8")?.to_string());
            let updated = app.clone();
            repository.commit().map_err(|error| error.message)?;
            Ok(updated)
        },
    )
}

pub fn save(directory: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let (extension, _) = image_format(bytes)?;
    let digest = format!("{:x}", Sha256::digest(bytes));
    let path = directory.join(format!("{digest}.{extension}"));
    persistence::write_atomic(&path, bytes).map_err(|error| error.message)?;
    Ok(path)
}

pub fn data_url(directory: &Path, path: &str) -> Result<String, String> {
    let directory = fs::canonicalize(directory).map_err(|error| error.to_string())?;
    let path = fs::canonicalize(path).map_err(|error| error.to_string())?;
    if !path.starts_with(&directory) {
        return Err("icon is outside the application icon directory".into());
    }
    let bytes = read_image(&path)?;
    let (_, mime) = image_format(&bytes)?;
    Ok(encode_data_url(mime, &bytes))
}

pub(crate) fn encode_data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64(bytes))
}

fn base64(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or_default();
        let third = chunk.get(2).copied().unwrap_or_default();
        result.push(BASE64_ALPHABET[(first >> 2) as usize] as char);
        result.push(BASE64_ALPHABET[(((first & 3) << 4) | (second >> 4)) as usize] as char);
        result.push(if chunk.len() > 1 {
            BASE64_ALPHABET[(((second & 15) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            BASE64_ALPHABET[(third & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}

#[cfg(test)]
#[path = "icons_tests.rs"]
mod tests;
