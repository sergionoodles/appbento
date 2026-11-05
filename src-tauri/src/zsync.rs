//! Resolve updates from a `.zsync` control file header; the payload is never downloaded here.
use crate::github::{self, ExpectedChecksum, ResolvedUpdate};

const MAX_HEADER_BYTES: usize = 16 * 1024;
const SHA1_HEX_LENGTH: usize = 40;
const FALLBACK_ASSET_NAME: &str = "update.AppImage";

#[derive(Debug, PartialEq, Eq)]
struct ControlHeader {
    sha1: String,
    url: String,
    length: Option<u64>,
    filename: Option<String>,
}

/// Collects streamed bytes until the blank line that ends the control-file header.
#[derive(Default)]
struct HeaderBuffer {
    bytes: Vec<u8>,
}

impl HeaderBuffer {
    fn push(&mut self, chunk: &[u8]) -> Result<Option<Vec<u8>>, String> {
        self.bytes.extend_from_slice(chunk);
        match header_end(&self.bytes) {
            Some(end) if end > MAX_HEADER_BYTES => Err(oversized()),
            Some(end) => Ok(Some(self.bytes[..end].to_vec())),
            None if self.bytes.len() > MAX_HEADER_BYTES => Err(oversized()),
            None => Ok(None),
        }
    }
}

fn oversized() -> String {
    "zsync control file header exceeds the 16 KiB limit".into()
}

/// Index where the blank separator line begins, i.e. just past the last header line.
fn header_end(bytes: &[u8]) -> Option<usize> {
    let mut line_start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        let line = &bytes[line_start..index];
        if line.is_empty() || line == b"\r" {
            return Some(line_start);
        }
        line_start = index + 1;
    }
    None
}

fn parse_header(bytes: &[u8]) -> Result<ControlHeader, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "zsync control file is not valid text")?;
    let mut sha1 = None;
    let mut url = None;
    let mut length = None;
    let mut filename = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "SHA-1" if sha1.is_none() => sha1 = Some(value.to_ascii_lowercase()),
            "URL" if url.is_none() => url = Some(value.to_owned()),
            "Length" if length.is_none() => {
                length = Some(value.parse::<u64>().map_err(|_| "invalid zsync Length")?)
            }
            "Filename" if filename.is_none() && !value.is_empty() => {
                filename = Some(value.to_owned())
            }
            _ => {}
        }
    }
    let sha1 = sha1.ok_or("zsync control file has no SHA-1")?;
    if sha1.len() != SHA1_HEX_LENGTH || !sha1.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("zsync SHA-1 must be 40 hexadecimal characters".into());
    }
    let url = url
        .filter(|url| !url.is_empty())
        .ok_or("zsync control file has no URL")?;
    Ok(ControlHeader {
        sha1,
        url,
        length: length.filter(|length| *length > 0),
        filename,
    })
}

fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && !name.chars().any(char::is_control)
}

fn resolve_header(control_url: &str, bytes: &[u8]) -> Result<ResolvedUpdate, String> {
    let header = parse_header(bytes)?;
    let base = github::validate_https_url(control_url)?;
    let download = base
        .join(&header.url)
        .map_err(|error| format!("invalid zsync URL: {error}"))?;
    github::validate_https_url(download.as_str())?;
    let asset_name = header
        .filename
        .filter(|name| is_plain_file_name(name))
        .or_else(|| {
            download
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| FALLBACK_ASSET_NAME.into());
    Ok(ResolvedUpdate {
        latest: String::new(),
        download_url: download.to_string(),
        asset_name,
        published_at: None,
        size_bytes: header.length,
        checksum: Some(ExpectedChecksum::Sha1Hex(header.sha1)),
    })
}

pub(crate) async fn resolve(control_url: &str) -> Result<ResolvedUpdate, String> {
    resolve_with_fetch(control_url, |url| async move { fetch_header(&url).await }).await
}

async fn resolve_with_fetch<F, Fetch>(
    control_url: &str,
    mut fetch: F,
) -> Result<ResolvedUpdate, String>
where
    F: FnMut(String) -> Fetch,
    Fetch: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    github::validate_https_url(control_url)?;
    let header = fetch(control_url.to_owned()).await?;
    resolve_header(control_url, &header)
}

async fn fetch_header(url: &str) -> Result<Vec<u8>, String> {
    let mut response = github::client()?
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let mut buffer = HeaderBuffer::default();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if let Some(header) = buffer.push(&chunk)? {
            return Ok(header);
        }
    }
    Err("zsync control file ended before its header was complete".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";

    fn control(extra: &str) -> Vec<u8> {
        format!(
            "zsync: 0.6.2\nFilename: App-2.0-x86_64.AppImage\nMTime: Wed, 01 Jan 2025 00:00:00 +0000\nBlocksize: 2048\nLength: 4096\nHash-Lengths: 2,2,5\nURL: App-2.0-x86_64.AppImage\nSHA-1: {SHA1}\n{extra}\n\0\x01binary"
        )
        .into_bytes()
    }

    fn header_of(bytes: &[u8]) -> Vec<u8> {
        HeaderBuffer::default().push(bytes).unwrap().unwrap()
    }

    #[test]
    fn resolves_relative_urls_against_the_control_file() {
        let resolved = resolve_header(
            "https://downloads.example/app/App.AppImage.zsync",
            &header_of(&control("")),
        )
        .unwrap();
        assert_eq!(
            resolved.download_url,
            "https://downloads.example/app/App-2.0-x86_64.AppImage"
        );
        assert_eq!(resolved.asset_name, "App-2.0-x86_64.AppImage");
        assert_eq!(resolved.size_bytes, Some(4096));
        assert_eq!(
            resolved.checksum,
            Some(ExpectedChecksum::Sha1Hex(SHA1.into()))
        );
        assert!(resolved.latest.is_empty());
    }

    #[test]
    fn accepts_absolute_https_urls_and_rejects_other_schemes() {
        let absolute = String::from_utf8(control("")).unwrap().replace(
            "URL: App-2.0-x86_64.AppImage",
            "URL: https://cdn.example/a/App.AppImage",
        );
        let resolved =
            resolve_header("https://downloads.example/App.zsync", absolute.as_bytes()).unwrap();
        assert_eq!(resolved.download_url, "https://cdn.example/a/App.AppImage");
        for url in [
            "http://cdn.example/App.AppImage",
            "file:///tmp/App.AppImage",
        ] {
            let unsafe_url = absolute.replace("https://cdn.example/a/App.AppImage", url);
            assert!(
                resolve_header("https://downloads.example/App.zsync", unsafe_url.as_bytes())
                    .is_err()
            );
        }
    }

    #[test]
    fn requires_a_valid_sha1_and_url() {
        let text = String::from_utf8(control("")).unwrap();
        let missing_hash = text.replace("SHA-1:", "X-Other:");
        let short_hash = text.replace(SHA1, "abc123");
        let non_hex_hash = text.replace(SHA1, &"g".repeat(40));
        let missing_url = text.replace("\nURL:", "\nX-Url:");
        let bad_length = text.replace("Length: 4096", "Length: many");
        for broken in [
            missing_hash,
            short_hash,
            non_hex_hash,
            missing_url,
            bad_length,
        ] {
            assert!(parse_header(broken.as_bytes()).is_err(), "{broken}");
        }
    }

    #[test]
    fn falls_back_to_the_url_name_for_unsafe_filenames() {
        let text = String::from_utf8(control(""))
            .unwrap()
            .replace("Filename: App-2.0-x86_64.AppImage", "Filename: ../evil");
        let resolved =
            resolve_header("https://downloads.example/App.zsync", text.as_bytes()).unwrap();
        assert_eq!(resolved.asset_name, "App-2.0-x86_64.AppImage");
    }

    #[test]
    fn stops_at_the_blank_line_across_chunks() {
        let bytes = control("");
        let mut buffer = HeaderBuffer::default();
        let (first, rest) = bytes.split_at(20);
        assert_eq!(buffer.push(first).unwrap(), None);
        let header = buffer.push(rest).unwrap().unwrap();
        assert!(header.ends_with(format!("SHA-1: {SHA1}\n").as_bytes()));
        assert!(!header.contains(&0));
    }

    #[test]
    fn rejects_oversized_headers() {
        let mut buffer = HeaderBuffer::default();
        let filler = vec![b'a'; MAX_HEADER_BYTES / 2];
        assert_eq!(buffer.push(&filler).unwrap(), None);
        assert_eq!(buffer.push(&filler).unwrap(), None);
        assert!(buffer.push(b"b").is_err());
        let mut late = vec![b'a'; MAX_HEADER_BYTES];
        late.extend_from_slice(b"\n\n");
        assert!(HeaderBuffer::default().push(&late).is_err());
    }

    #[tokio::test]
    async fn resolves_through_an_injected_fetch() {
        let mut requested = Vec::new();
        let resolved = resolve_with_fetch("https://downloads.example/App.zsync", |url| {
            requested.push(url);
            async { Ok(header_of(&control(""))) }
        })
        .await
        .unwrap();
        assert_eq!(requested, vec!["https://downloads.example/App.zsync"]);
        assert_eq!(resolved.asset_name, "App-2.0-x86_64.AppImage");
        assert!(
            resolve_with_fetch("http://downloads.example/App.zsync", |_| async {
                Ok(Vec::new())
            })
            .await
            .is_err()
        );
    }
}
