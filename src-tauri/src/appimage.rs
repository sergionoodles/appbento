//! Validate AppImage files without executing them.
use crate::models::AppKind;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const ELF_HEADER_LENGTH: usize = 20;

pub fn open_appimage(path: &Path) -> Result<File, String> {
    super::app_file::open_app(path, AppKind::AppImage)
}

pub(super) fn validate(file: &mut File) -> Result<(), String> {
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
    Ok(())
}
