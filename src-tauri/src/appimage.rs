//! Validate AppImage files without executing them.
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

const NO_FOLLOW: i32 = 0x20000;
const NONBLOCKING: i32 = 0x800;
const ELF_HEADER_LENGTH: usize = 20;

pub fn open_appimage(path: &Path) -> Result<File, String> {
    if !path.is_absolute()
        || path.to_str().is_none()
        || path.to_string_lossy().chars().any(char::is_control)
    {
        return Err("AppImage path must be absolute UTF-8 without control characters".into());
    }
    if !std::fs::symlink_metadata(path)
        .map_err(|error| file_access_error(path, error))?
        .file_type()
        .is_file()
    {
        return Err("AppImage must be a regular non-symlink file".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(NO_FOLLOW | NONBLOCKING)
        .open(path)
        .map_err(|error| file_access_error(path, error))?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("AppImage must be a regular non-symlink file".into());
    }
    let mut header = [0; ELF_HEADER_LENGTH];
    file.read_exact(&mut header)
        .map_err(|_| "invalid AppImage header")?;
    if header[4] != 2
        || &header[..4] != b"\x7fELF"
        || &header[8..10] != b"AI"
        || ![1, 2].contains(&header[10])
    {
        return Err("file is not an AppImage".into());
    }
    let architecture = match header[5] {
        1 => u16::from_le_bytes([header[18], header[19]]),
        2 => u16::from_be_bytes([header[18], header[19]]),
        _ => return Err("invalid ELF byte order".into()),
    };
    let expected = match std::env::consts::ARCH {
        "x86_64" => 62,
        "aarch64" => 183,
        _ => return Err("unsupported host architecture".into()),
    };
    if architecture != expected {
        return Err("AppImage architecture does not match this computer".into());
    }
    use std::io::Seek;
    file.rewind().map_err(|error| error.to_string())?;
    Ok(file)
}

fn file_access_error(path: &Path, error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        return format!(
            "AppImage file is missing: {}. If it was moved or replaced, scan its current folder and remove the old library entry.",
            path.display()
        );
    }

    format!("cannot access AppImage {}: {error}", path.display())
}
