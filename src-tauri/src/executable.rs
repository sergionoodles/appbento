//! Bounded ELF parsing for host-native Linux executables, without running them.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

const HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: u16 = 56;
const MAX_PROGRAM_HEADERS: u16 = 1024;
const MAX_LOADER_PATH: u64 = 4096;
const PT_INTERP: u32 = 3;

pub(super) fn validate(file: &mut File) -> Result<Option<PathBuf>, String> {
    file.rewind().map_err(|error| error.to_string())?;
    let mut header = [0; HEADER_SIZE];
    let count = file.read(&mut header).map_err(|error| error.to_string())?;
    if header.starts_with(b"#!") {
        return Err("Scripts are not supported yet; select a native Linux executable".into());
    }
    if count != HEADER_SIZE || &header[..4] != b"\x7fELF" {
        return Err("file is not a supported Linux executable".into());
    }
    if header[4] != 2
        || header[5] != 1
        || header[6] != 1
        || ![0, 3].contains(&header[7])
        || read_u32(&header, 20) != 1
    {
        return Err("Executable must use 64-bit little-endian ELF version 1 with System V or GNU/Linux OS ABI".into());
    }
    if ![2, 3].contains(&read_u16(&header, 16)) {
        return Err("ELF file must be an executable or position-independent executable".into());
    }
    let expected = match std::env::consts::ARCH {
        "x86_64" => 62,
        "aarch64" => 183,
        _ => return Err("unsupported host architecture".into()),
    };
    if read_u16(&header, 18) != expected {
        return Err("Executable architecture does not match this computer".into());
    }
    let offset = read_u64(&header, 32);
    let entry_size = read_u16(&header, 54);
    let count = read_u16(&header, 56);
    let file_size = file.metadata().map_err(|error| error.to_string())?.len();
    let table_end = offset.checked_add(u64::from(count) * u64::from(entry_size));
    if entry_size != PROGRAM_HEADER_SIZE
        || count > MAX_PROGRAM_HEADERS
        || table_end.is_none_or(|end| end > file_size)
        || (count > 0 && offset < HEADER_SIZE as u64)
    {
        return Err("invalid or oversized ELF program header table".into());
    }
    let mut loader = None;
    for index in 0..count {
        file.seek(SeekFrom::Start(
            offset + u64::from(index) * u64::from(entry_size),
        ))
        .map_err(|error| error.to_string())?;
        let mut program = [0; PROGRAM_HEADER_SIZE as usize];
        file.read_exact(&mut program)
            .map_err(|_| "truncated ELF program header")?;
        if read_u32(&program, 0) != PT_INTERP {
            continue;
        }
        if loader.is_some() {
            return Err("multiple ELF program loaders are not supported".into());
        }
        let start = read_u64(&program, 8);
        let length = read_u64(&program, 32);
        if !(2..=MAX_LOADER_PATH).contains(&length)
            || start.checked_add(length).is_none_or(|end| end > file_size)
        {
            return Err("invalid ELF program loader path".into());
        }
        file.seek(SeekFrom::Start(start))
            .map_err(|error| error.to_string())?;
        let mut bytes = vec![0; length as usize];
        file.read_exact(&mut bytes)
            .map_err(|_| "truncated ELF program loader path")?;
        if bytes.pop() != Some(0) || bytes.contains(&0) {
            return Err("ELF program loader path must be NUL-terminated".into());
        }
        let path =
            PathBuf::from(String::from_utf8(bytes).map_err(|_| "invalid ELF program loader path")?);
        if !path.is_absolute() || path.to_string_lossy().chars().any(char::is_control) {
            return Err("ELF program loader path must be absolute".into());
        }
        if !path.is_file() {
            return Err(format!(
                "requires the program loader {}, which is not installed",
                path.display()
            ));
        }
        loader = Some(path);
    }
    Ok(loader)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("bounded ELF field"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("bounded ELF field"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("bounded ELF field"),
    )
}
