//! Shared safe file opening, format dispatch, and review hashes. Never executes a file.
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::models::AppKind;
use sha2::{Digest, Sha256};

const NO_FOLLOW: i32 = 0x20000;
const NONBLOCKING: i32 = 0x800;

fn open_regular(path: &Path, kind: AppKind) -> Result<File, String> {
    let label = if kind == AppKind::AppImage {
        "AppImage"
    } else {
        "Executable"
    };
    if !path.is_absolute()
        || path.to_str().is_none()
        || path.to_string_lossy().chars().any(char::is_control)
    {
        return Err(format!(
            "{label} path must be absolute UTF-8 without control characters"
        ));
    }
    if !std::fs::symlink_metadata(path)
        .map_err(|error| access_error(path, kind, error))?
        .file_type()
        .is_file()
    {
        return Err(format!("{label} must be a regular non-symlink file"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(NO_FOLLOW | NONBLOCKING)
        .open(path)
        .map_err(|error| access_error(path, kind, error))?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err(format!("{label} must be a regular non-symlink file"));
    }
    Ok(file)
}

fn access_error(path: &Path, kind: AppKind, error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        if kind == AppKind::Binary {
            return format!("Executable file is missing: {}. If it was moved or renamed, add it again from its new location and remove the old library entry. Its new entry will have new permissions and private data.", path.display());
        }
        return format!("AppImage file is missing: {}. If it was moved or replaced, scan its current folder and remove the old library entry.", path.display());
    }
    let label = if kind == AppKind::AppImage {
        "AppImage"
    } else {
        "executable"
    };
    format!("cannot access {label} {}: {error}", path.display())
}

fn has_appimage_magic(file: &mut File) -> Result<bool, String> {
    let mut header = [0; 10];
    let count = file.read(&mut header).map_err(|error| error.to_string())?;
    file.rewind().map_err(|error| error.to_string())?;
    Ok(count == header.len() && &header[8..10] == b"AI")
}

pub fn detect(path: &Path) -> Result<AppKind, String> {
    let mut file = open_regular(path, AppKind::Binary)?;
    let kind = if has_appimage_magic(&mut file)? {
        AppKind::AppImage
    } else {
        AppKind::Binary
    };
    validate(&mut file, kind)?;
    Ok(kind)
}

pub fn open_app(path: &Path, kind: AppKind) -> Result<File, String> {
    let mut file = open_regular(path, kind)?;
    validate(&mut file, kind)?;
    Ok(file)
}

fn validate(file: &mut File, kind: AppKind) -> Result<(), String> {
    match kind {
        AppKind::AppImage => super::appimage::validate(file)?,
        AppKind::Binary => {
            if has_appimage_magic(file)? {
                return Err("file is an AppImage, not a native executable".into());
            }
            super::executable::validate(file)?;
        }
    }
    file.rewind().map_err(|error| error.to_string())
}

pub fn app_digest(path: &Path, kind: AppKind) -> Result<String, String> {
    digest_file(&mut open_app(path, kind)?)
}

pub fn appimage_digest(path: &Path) -> Result<String, String> {
    app_digest(path, AppKind::AppImage)
}

pub fn digest_file(file: &mut File) -> Result<String, String> {
    file.rewind().map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub fn program_loader(file: &mut File) -> Result<Option<std::path::PathBuf>, String> {
    super::executable::validate(file)
}
