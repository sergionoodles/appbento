//! Read Type 2 AppImage metadata without executing the AppImage or extracting its filesystem.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::{Component, Path};
use std::process::{Command, Stdio};
use std::sync::{mpsc, LazyLock};
use std::time::{Duration, Instant};

use regex::Regex;

const RUNTIME_LIMIT: usize = 16 * 1024 * 1024;
const LIST_LIMIT: usize = 1024 * 1024;
const DESKTOP_LIMIT: usize = 64 * 1024;
const APPSTREAM_LIMIT: usize = 256 * 1024;
const ICON_LIMIT: usize = 5 * 1024 * 1024;
const CANDIDATE_LIMIT: usize = 8;
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const LIST_PREFIX: &str = "squashfs-root/";
/// Directories below the AppImage root that hold icons and AppStream metadata. Listing only
/// these keeps large AppImages (thousands of bundled files) within `LIST_LIMIT`.
const METADATA_DIRECTORIES: [&str; 7] = [
    "usr/share/icons",
    "usr/share/pixmaps",
    "usr/share/metainfo",
    "usr/share/appdata",
    "share/icons",
    "share/metainfo",
    "share/appdata",
];

#[derive(Debug, Default)]
pub struct EmbeddedMetadata {
    pub icon: Option<Vec<u8>>,
    pub website: Option<String>,
}

/// Name and version as the AppImage declares them, before any cleanup.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EmbeddedIdentity {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct DesktopEntry {
    icon: Option<String>,
    website: Option<String>,
    name: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct AppStreamComponent {
    name: Option<String>,
    version: Option<String>,
}

struct SquashfsImage {
    // Keep the validated inode open so a concurrent path replacement cannot change the input.
    _file: File,
    source: String,
    offset: u64,
    paths: Vec<String>,
    deadline: Instant,
}

impl SquashfsImage {
    fn open(path: &Path) -> Result<Self, String> {
        let mut file = crate::lifecycle::open_appimage(path)?;
        let offset = squashfs_offset(&mut file)?;
        let source = format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd());
        let deadline = Instant::now() + DISCOVERY_TIMEOUT;
        let root = run_unsquashfs(
            &source,
            offset,
            &["-ls", "-max-depth", "1"],
            &[],
            LIST_LIMIT,
            deadline,
        )?;
        let nested = run_unsquashfs(
            &source,
            offset,
            &["-ls"],
            &METADATA_DIRECTORIES,
            LIST_LIMIT,
            deadline,
        )?;
        let mut paths = parse_listing(&String::from_utf8_lossy(&root));
        paths.extend(parse_listing(&String::from_utf8_lossy(&nested)));
        Ok(Self {
            _file: file,
            source,
            offset,
            paths,
            deadline,
        })
    }

    /// Reads an entry, skipping unreadable ones unless the overall deadline has passed.
    fn read(&self, entry: &str, limit: usize) -> Result<Option<Vec<u8>>, String> {
        match run_unsquashfs(
            &self.source,
            self.offset,
            &["-cat", "-follow"],
            &[entry],
            limit,
            self.deadline,
        ) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if Instant::now() >= self.deadline => Err(error),
            Err(_) => Ok(None),
        }
    }

    fn desktop_entries(&self) -> Result<Vec<DesktopEntry>, String> {
        let mut entries = Vec::new();
        for desktop in self
            .paths
            .iter()
            .filter(|path| !path.contains('/') && path.ends_with(".desktop"))
            .take(CANDIDATE_LIMIT)
        {
            if let Some(bytes) = self.read(desktop, DESKTOP_LIMIT)? {
                entries.push(desktop_metadata(&String::from_utf8_lossy(&bytes)));
            }
        }
        Ok(entries)
    }

    fn appstream_components(&self) -> Result<Vec<AppStreamComponent>, String> {
        let mut components = Vec::new();
        for metadata in self
            .paths
            .iter()
            .filter(|path| is_appstream_path(path))
            .take(CANDIDATE_LIMIT)
        {
            if let Some(bytes) = self.read(metadata, APPSTREAM_LIMIT)? {
                components.push(appstream_metadata(&String::from_utf8_lossy(&bytes)));
            }
        }
        Ok(components)
    }
}

pub fn discover(path: &Path) -> Result<EmbeddedMetadata, String> {
    let image = SquashfsImage::open(path)?;
    let mut metadata = EmbeddedMetadata::default();
    let mut icon_names = Vec::new();
    for entry in image.desktop_entries()? {
        metadata.website = metadata.website.or(entry.website);
        if let Some(icon) = entry.icon {
            icon_names.push(icon);
        }
    }
    let mut candidates = icon_candidates(&image.paths, &icon_names);
    candidates.sort_by_key(|path| path.contains('/'));
    if image.paths.iter().any(|path| path == ".DirIcon") {
        candidates.truncate(CANDIDATE_LIMIT - 1);
        candidates.push(".DirIcon".into());
    }
    for candidate in candidates.into_iter().take(CANDIDATE_LIMIT) {
        let Some(bytes) = image.read(&candidate, ICON_LIMIT)? else {
            continue;
        };
        if crate::icons::image_format(&bytes).is_ok() {
            metadata.icon = Some(bytes);
            break;
        }
    }
    Ok(metadata)
}

/// Prefers the AppStream name (the canonical product name; desktop entries often add
/// qualifiers such as "Portable") and the explicit AppImage version over release history.
pub fn discover_identity(path: &Path) -> Result<EmbeddedIdentity, String> {
    let image = SquashfsImage::open(path)?;
    let desktop_entries = image.desktop_entries()?;
    let components = image.appstream_components()?;
    let name = components
        .iter()
        .find_map(|component| component.name.clone())
        .or_else(|| desktop_entries.iter().find_map(|entry| entry.name.clone()));
    let version = desktop_entries
        .iter()
        .find_map(|entry| entry.version.clone())
        .or_else(|| {
            components
                .iter()
                .find_map(|component| component.version.clone())
        });
    Ok(EmbeddedIdentity { name, version })
}

fn squashfs_offset(file: &mut File) -> Result<u64, String> {
    let size = file.metadata().map_err(|error| error.to_string())?.len();
    let mut runtime = Vec::new();
    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    file.take((RUNTIME_LIMIT + 96) as u64)
        .read_to_end(&mut runtime)
        .map_err(|error| error.to_string())?;
    if runtime.get(10) != Some(&2) {
        return Err("embedded icons currently require a Type 2 AppImage".into());
    }
    runtime
        .windows(96)
        .enumerate()
        .find_map(|(offset, header)| {
            if offset > RUNTIME_LIMIT || &header[..4] != b"hsqs" {
                return None;
            }
            let block_size = u32::from_le_bytes(header[12..16].try_into().ok()?);
            let compression = u16::from_le_bytes(header[20..22].try_into().ok()?);
            let major = u16::from_le_bytes(header[28..30].try_into().ok()?);
            let minor = u16::from_le_bytes(header[30..32].try_into().ok()?);
            let used = u64::from_le_bytes(header[40..48].try_into().ok()?);
            (major == 4
                && minor == 0
                && (1..=6).contains(&compression)
                && block_size.is_power_of_two()
                && (4096..=1048576).contains(&block_size)
                && used >= 96
                && used <= size.saturating_sub(offset as u64))
            .then_some(offset as u64)
        })
        .ok_or_else(|| {
            "no supported embedded SquashFS found within the AppImage runtime limit".into()
        })
}

fn run_unsquashfs(
    source: &str,
    offset: u64,
    options: &[&str],
    paths: &[&str],
    limit: usize,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    if Instant::now() >= deadline {
        return Err("embedded icon discovery timed out".into());
    }
    let offset = offset.to_string();
    let mut child = Command::new("/usr/bin/unsquashfs")
        .args([
            "-no-wildcards",
            "-processors",
            "1",
            "-mem",
            "16M",
            "-offset",
            &offset,
        ])
        .args(options)
        .arg(source)
        .args(paths)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!("cannot read embedded AppImage metadata with unsquashfs: {error}")
        })?;
    let stdout = child.stdout.take().ok_or("missing unsquashfs output")?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())
            .and_then(|_| {
                if bytes.len() > limit {
                    return Err("embedded metadata exceeds the size limit".into());
                }
                Ok(bytes)
            });
        let _ = sender.send(result);
    });
    let mut output = None;
    loop {
        if let Ok(result) = receiver.try_recv() {
            match result {
                Ok(bytes) => output = Some(bytes),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err("cannot read the embedded SquashFS entry".into());
                }
                return match output {
                    Some(bytes) => Ok(bytes),
                    None => receiver
                        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                        .map_err(|_| "embedded icon discovery timed out".to_owned())?,
                };
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("embedded icon discovery timed out".into());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn is_safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('-')
        && !path.chars().any(char::is_control)
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn parse_listing(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| line.strip_prefix(LIST_PREFIX))
        .filter(|path| is_safe_path(path))
        .map(str::to_owned)
        .collect()
}

fn desktop_metadata(desktop: &str) -> DesktopEntry {
    let mut in_entry = false;
    let mut entry = DesktopEntry::default();
    for line in desktop.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Icon" if is_safe_path(value) => entry.icon = Some(value.to_owned()),
            "URL" | "Website" | "X-AppImage-URL" | "X-AppImage-Website"
                if value.starts_with("https://") || value.starts_with("http://") =>
            {
                entry.website = Some(value.to_owned())
            }
            // `Version` is the Desktop Entry specification version, not the app version.
            "Name" if !value.is_empty() => entry.name = Some(value.to_owned()),
            "X-AppImage-Version" if !value.is_empty() => entry.version = Some(value.to_owned()),
            _ => {}
        }
    }
    entry
}

fn is_appstream_path(path: &str) -> bool {
    let is_metadata_directory = ["metainfo/", "appdata/"].iter().any(|directory| {
        path.starts_with(&format!("usr/share/{directory}"))
            || path.starts_with(&format!("share/{directory}"))
    });
    is_metadata_directory && path.ends_with(".xml")
}

static APPSTREAM_DEVELOPER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<developer\b.*?</developer>").expect("valid regex"));
// Only the untranslated `<name>` has no attributes; translations carry `xml:lang`.
static APPSTREAM_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<name>\s*([^<]+?)\s*</name>").expect("valid regex"));
// AppStream lists releases newest first.
static APPSTREAM_RELEASE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<release\b[^>]*?\sversion\s*=\s*["']([^"']+)["']"#).expect("valid regex")
});

fn appstream_metadata(xml: &str) -> AppStreamComponent {
    let component = APPSTREAM_DEVELOPER.replace_all(xml, "");
    let capture = |expression: &Regex| {
        expression
            .captures(&component)
            .and_then(|captures| captures.get(1))
            .map(|value| decode_xml_entities(value.as_str()))
    };
    AppStreamComponent {
        name: capture(&APPSTREAM_NAME),
        version: capture(&APPSTREAM_RELEASE),
    }
}

fn decode_xml_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn icon_candidates(paths: &[String], names: &[String]) -> Vec<String> {
    let mut candidates = Vec::new();
    for name in names {
        for path in paths {
            let file = Path::new(path)
                .file_name()
                .and_then(|file| file.to_str())
                .unwrap_or_default();
            let stem = Path::new(file)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default();
            let extension = Path::new(file)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if matches!(
                extension,
                "png" | "svg" | "ico" | "jpg" | "jpeg" | "webp" | "gif"
            ) && (path == name || file == name || stem == name)
                && !candidates.contains(path)
            {
                candidates.push(path.clone());
            }
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn parses_only_safe_desktop_entry_metadata() {
        assert_eq!(desktop_metadata("[Desktop Entry]\nName=Sample\nName[de]=Beispiel\nVersion=1.0\nX-AppImage-Version=2.3.4\nIcon=sample\nX-AppImage-URL=https://example.org\n[Other]\nIcon=wrong\nName=Wrong"),
            DesktopEntry {
                icon: Some("sample".into()),
                website: Some("https://example.org".into()),
                name: Some("Sample".into()),
                version: Some("2.3.4".into()),
            });
        for icon in ["../outside", "/absolute", "-option", "folder/../outside"] {
            assert_eq!(
                desktop_metadata(&format!("[Desktop Entry]\nIcon={icon}")),
                DesktopEntry::default()
            );
        }
    }

    #[test]
    fn parses_the_component_name_and_newest_release() {
        let xml = r#"<?xml version="1.0"?>
<component type="desktop-application">
  <developer id="org.example"><name>Example Developers</name></developer>
  <name xml:lang="de">Beispiel</name>
  <name>Sample &amp; Co</name>
  <releases>
    <release version="4.0.1" date="2026-09-23"/>
    <release version="4.0.0" date="2026-08-01"/>
  </releases>
</component>"#;
        assert_eq!(
            appstream_metadata(xml),
            AppStreamComponent {
                name: Some("Sample & Co".into()),
                version: Some("4.0.1".into()),
            }
        );
        assert!(is_appstream_path("share/metainfo/org.example.appdata.xml"));
        assert!(is_appstream_path(
            "usr/share/metainfo/org.example.metainfo.xml"
        ));
        assert!(!is_appstream_path("usr/share/icons/index.xml"));
    }

    #[test]
    fn selects_named_icons_without_unsafe_listing_paths() {
        let paths = parse_listing("squashfs-root/../outside\nsquashfs-root/-option\nsquashfs-root/sample.desktop\nsquashfs-root/usr/share/icons/hicolor/256x256/apps/sample.svg\nsquashfs-root/.DirIcon");
        assert_eq!(
            icon_candidates(&paths, &["sample".into()]),
            vec!["usr/share/icons/hicolor/256x256/apps/sample.svg"]
        );
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn discovers_real_appimage_icon_and_diricon_symlink() {
        if !Path::new("/usr/bin/mksquashfs").exists() || !Path::new("/usr/bin/unsquashfs").exists()
        {
            eprintln!("skipping fixture: squashfs-tools unavailable");
            return;
        }
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let temporary = std::env::temp_dir().join(format!(
            "appbento-embedded-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(temporary.join("root/usr/share/icons")).unwrap();
        let icon = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><rect width=\"1\" height=\"1\"/></svg>";
        fs::write(temporary.join("root/usr/share/icons/sample.svg"), icon).unwrap();
        let fallback = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2\" height=\"2\"/>";
        fs::write(temporary.join("root/fallback.svg"), fallback).unwrap();
        fs::write(
            temporary.join("root/sample.desktop"),
            "[Desktop Entry]\nName=Sample Portable\nVersion=1.0\nIcon=sample\nURL=https://example.org\n",
        )
        .unwrap();
        symlink("fallback.svg", temporary.join("root/.DirIcon")).unwrap();
        fs::create_dir_all(temporary.join("root/usr/share/metainfo")).unwrap();
        fs::write(
            temporary.join("root/usr/share/metainfo/org.example.Sample.metainfo.xml"),
            "<component><name>Sample</name><releases><release version=\"2.0.0\"/></releases></component>",
        )
        .unwrap();
        // Enough bundled files that a full listing would exceed LIST_LIMIT.
        let bundled = temporary.join("root/lib/bundled");
        fs::create_dir_all(&bundled).unwrap();
        for index in 0..(LIST_LIMIT / 200 + 1) {
            fs::write(bundled.join(format!("{index:0>200}")), b"").unwrap();
        }
        for named_icon in [true, false] {
            if !named_icon {
                fs::write(
                    temporary.join("root/sample.desktop"),
                    "[Desktop Entry]\nIcon=missing\nURL=https://example.org\n",
                )
                .unwrap();
            }
            let status = Command::new("/usr/bin/mksquashfs")
                .arg(temporary.join("root"))
                .arg(temporary.join("filesystem"))
                .args(["-noappend", "-processors", "1", "-quiet"])
                .stdout(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
            let mut image = vec![0; 4096];
            image[..4].copy_from_slice(b"\x7fELF");
            image[4] = 2;
            image[5] = 1;
            image[8..11].copy_from_slice(b"AI\x02");
            let architecture: u16 = if std::env::consts::ARCH == "aarch64" {
                183
            } else {
                62
            };
            image[18..20].copy_from_slice(&architecture.to_le_bytes());
            image.extend(fs::read(temporary.join("filesystem")).unwrap());
            let path = temporary.join("sample.AppImage");
            fs::write(&path, image).unwrap();
            let metadata = discover(&path).unwrap();
            assert_eq!(
                metadata.icon.as_deref(),
                Some(if named_icon {
                    icon.as_slice()
                } else {
                    fallback.as_slice()
                })
            );
            assert_eq!(metadata.website.as_deref(), Some("https://example.org"));
            assert_eq!(
                discover_identity(&path).unwrap(),
                EmbeddedIdentity {
                    name: Some("Sample".into()),
                    version: Some("2.0.0".into()),
                }
            );
        }
        let source = temporary.join("filesystem");
        assert!(run_unsquashfs(
            source.to_str().unwrap(),
            0,
            &["-cat"],
            &["usr/share/icons/sample.svg"],
            4,
            Instant::now() + DISCOVERY_TIMEOUT
        )
        .unwrap_err()
        .contains("size limit"));
        let path = temporary.join("sample.AppImage");
        let mut image = fs::read(&path).unwrap();
        image[10] = 1;
        fs::write(&path, image).unwrap();
        assert!(discover(&path).unwrap_err().contains("Type 2"));
        fs::remove_dir_all(temporary).unwrap();
    }
}
