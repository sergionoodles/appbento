//! Bounded ELF section reader that extracts an AppImage's `.upd_info` without running it.
use std::io::{Read, Seek, SeekFrom};

const HEADER_SIZE: usize = 64;
const SECTION_HEADER_SIZE: u16 = 64;
const MAX_SECTION_HEADERS: u16 = 512;
const MAX_SECTION_NAMES: u64 = 64 * 1024;
const MAX_UPD_INFO: u64 = 4096;
const UPD_INFO_SECTION: &[u8] = b".upd_info";
const SECTION_TYPE_NOBITS: u32 = 8;
const APPIMAGE_TYPE_OFFSET: usize = 10;
const APPIMAGE_TYPE_2: u8 = 2;

struct SectionHeader {
    name: u32,
    kind: u32,
    offset: u64,
    size: u64,
}

/// Returns the NUL-trimmed `.upd_info` text, or `None` when the file is not a Type 2 AppImage
/// or the section is absent or empty.
pub(crate) fn read_upd_info<R: Read + Seek>(reader: &mut R) -> Result<Option<String>, String> {
    let file_size = reader
        .seek(SeekFrom::End(0))
        .map_err(|error| error.to_string())?;
    reader.rewind().map_err(|error| error.to_string())?;
    let mut header = [0; HEADER_SIZE];
    if reader.read_exact(&mut header).is_err() || &header[..4] != b"\x7fELF" {
        return Ok(None);
    }
    if header[4] != 2 || header[5] != 1 || header[APPIMAGE_TYPE_OFFSET] != APPIMAGE_TYPE_2 {
        return Ok(None);
    }
    let table_offset = read_u64(&header, 40);
    let entry_size = read_u16(&header, 58);
    let count = read_u16(&header, 60);
    let names_index = read_u16(&header, 62);
    if table_offset == 0 || count == 0 {
        return Ok(None);
    }
    if entry_size != SECTION_HEADER_SIZE
        || count > MAX_SECTION_HEADERS
        || names_index >= count
        || !fits(
            table_offset,
            u64::from(count) * u64::from(entry_size),
            file_size,
        )
    {
        return Err("invalid or oversized ELF section header table".into());
    }
    let sections = read_section_headers(reader, table_offset, count)?;
    let names = &sections[usize::from(names_index)];
    let names = read_range(
        reader,
        names.offset,
        names.size,
        MAX_SECTION_NAMES,
        file_size,
    )?;
    let Some(section) = sections
        .iter()
        .find(|section| section_name(&names, section.name) == Some(UPD_INFO_SECTION))
    else {
        return Ok(None);
    };
    if section.kind == SECTION_TYPE_NOBITS {
        return Ok(None);
    }
    let bytes = read_range(
        reader,
        section.offset,
        section.size,
        MAX_UPD_INFO,
        file_size,
    )?;
    let text = bytes.split(|byte| *byte == 0).next().unwrap_or_default();
    let text = std::str::from_utf8(text).map_err(|_| "ELF .upd_info is not valid UTF-8")?;
    let text = text.trim();
    Ok((!text.is_empty()).then(|| text.to_owned()))
}

fn read_section_headers<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    count: u16,
) -> Result<Vec<SectionHeader>, String> {
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut table = vec![0; usize::from(count) * usize::from(SECTION_HEADER_SIZE)];
    reader
        .read_exact(&mut table)
        .map_err(|_| "truncated ELF section header table")?;
    Ok(table
        .chunks_exact(usize::from(SECTION_HEADER_SIZE))
        .map(|entry| SectionHeader {
            name: read_u32(entry, 0),
            kind: read_u32(entry, 4),
            offset: read_u64(entry, 24),
            size: read_u64(entry, 32),
        })
        .collect())
}

fn read_range<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    size: u64,
    limit: u64,
    file_size: u64,
) -> Result<Vec<u8>, String> {
    if size > limit || !fits(offset, size, file_size) {
        return Err("ELF section is oversized or outside the file".into());
    }
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut bytes = vec![0; size as usize];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| "truncated ELF section")?;
    Ok(bytes)
}

fn fits(offset: u64, size: u64, file_size: u64) -> bool {
    offset.checked_add(size).is_some_and(|end| end <= file_size)
}

fn section_name(names: &[u8], offset: u32) -> Option<&[u8]> {
    let tail = names.get(offset as usize..)?;
    let length = tail.iter().position(|byte| *byte == 0)?;
    Some(&tail[..length])
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

/// Builds a minimal Type 2 AppImage-style ELF with a `.upd_info` section (when given).
#[cfg(test)]
pub(crate) fn synthetic_elf(upd_info: Option<&[u8]>) -> Vec<u8> {
    const NAMES: &[u8] = b"\0.shstrtab\0.upd_info\0";
    let mut image = vec![0; HEADER_SIZE];
    image[..4].copy_from_slice(b"\x7fELF");
    image[4] = 2;
    image[5] = 1;
    image[6] = 1;
    image[8..11].copy_from_slice(b"AI\x02");
    let architecture: u16 = if std::env::consts::ARCH == "aarch64" {
        183
    } else {
        62
    };
    image[18..20].copy_from_slice(&architecture.to_le_bytes());
    let names_offset = image.len() as u64;
    image.extend_from_slice(NAMES);
    let info_offset = image.len() as u64;
    image.extend_from_slice(upd_info.unwrap_or_default());
    let table_offset = (image.len() as u64).next_multiple_of(8);
    image.resize(table_offset as usize, 0);
    let section = |name: u32, kind: u32, offset: u64, size: u64| {
        let mut entry = [0_u8; SECTION_HEADER_SIZE as usize];
        entry[0..4].copy_from_slice(&name.to_le_bytes());
        entry[4..8].copy_from_slice(&kind.to_le_bytes());
        entry[24..32].copy_from_slice(&offset.to_le_bytes());
        entry[32..40].copy_from_slice(&size.to_le_bytes());
        entry
    };
    image.extend_from_slice(&section(0, 0, 0, 0));
    image.extend_from_slice(&section(1, 3, names_offset, NAMES.len() as u64));
    if let Some(info) = upd_info {
        image.extend_from_slice(&section(11, 1, info_offset, info.len() as u64));
    }
    let count: u16 = if upd_info.is_some() { 3 } else { 2 };
    image[40..48].copy_from_slice(&table_offset.to_le_bytes());
    image[58..60].copy_from_slice(&SECTION_HEADER_SIZE.to_le_bytes());
    image[60..62].copy_from_slice(&count.to_le_bytes());
    image[62..64].copy_from_slice(&1_u16.to_le_bytes());
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read(image: Vec<u8>) -> Result<Option<String>, String> {
        read_upd_info(&mut Cursor::new(image))
    }

    fn padded(text: &str) -> Vec<u8> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.resize(1024, 0);
        bytes
    }

    #[test]
    fn reads_the_nul_padded_update_information() {
        let info = "gh-releases-zsync|owner|repo|latest|repo-*.AppImage.zsync";
        assert_eq!(
            read(synthetic_elf(Some(&padded(info)))).unwrap().as_deref(),
            Some(info)
        );
    }

    #[test]
    fn treats_missing_and_empty_sections_as_absent() {
        assert_eq!(read(synthetic_elf(None)).unwrap(), None);
        assert_eq!(read(synthetic_elf(Some(&padded("")))).unwrap(), None);
        assert_eq!(read(synthetic_elf(Some(b""))).unwrap(), None);
    }

    #[test]
    fn ignores_type_1_appimages_and_other_files() {
        let mut type_1 = synthetic_elf(Some(&padded("guess")));
        type_1[10] = 1;
        assert_eq!(read(type_1).unwrap(), None);
        assert_eq!(read(b"#!/bin/sh\n".to_vec()).unwrap(), None);
        assert_eq!(read(Vec::new()).unwrap(), None);
        let mut big_endian = synthetic_elf(Some(&padded("guess")));
        big_endian[5] = 2;
        assert_eq!(read(big_endian).unwrap(), None);
    }

    fn mutated(change: impl FnOnce(&mut Vec<u8>, usize)) -> Result<Option<String>, String> {
        let mut image = synthetic_elf(Some(&padded("guess")));
        let table = u64::from_le_bytes(image[40..48].try_into().unwrap()) as usize;
        change(&mut image, table);
        read(image)
    }

    #[test]
    fn rejects_malformed_section_tables() {
        assert!(
            mutated(|image, _| image[40..48].copy_from_slice(&u64::MAX.to_le_bytes())).is_err()
        );
        assert!(
            mutated(|image, _| image[40..48].copy_from_slice(&1_000_000_u64.to_le_bytes()))
                .is_err()
        );
        assert!(mutated(|image, _| image[58..60].copy_from_slice(&40_u16.to_le_bytes())).is_err());
        assert!(
            mutated(|image, _| image[60..62].copy_from_slice(&u16::MAX.to_le_bytes())).is_err()
        );
        assert!(mutated(|image, _| image[60..62].copy_from_slice(&600_u16.to_le_bytes())).is_err());
        assert!(mutated(|image, _| image[62..64].copy_from_slice(&9_u16.to_le_bytes())).is_err());
    }

    #[test]
    fn rejects_sections_outside_the_file_or_over_their_limits() {
        // Section 2 is `.upd_info`; its offset and size live at +24 and +32 in the entry.
        let section_field = |table: usize, field: usize| table + 2 * 64 + field;
        assert!(mutated(|image, table| {
            let at = section_field(table, 24);
            image[at..at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        })
        .is_err());
        assert!(mutated(|image, table| {
            let at = section_field(table, 32);
            image[at..at + 8].copy_from_slice(&(MAX_UPD_INFO + 1).to_le_bytes());
        })
        .is_err());
        assert!(mutated(|image, table| {
            let at = section_field(table, 32);
            image[at..at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        })
        .is_err());
        // The section-name table is the second entry.
        assert!(mutated(|image, table| {
            let at = table + 64 + 32;
            image[at..at + 8].copy_from_slice(&(MAX_SECTION_NAMES + 1).to_le_bytes());
        })
        .is_err());
    }

    #[test]
    fn rejects_non_utf8_text_and_truncated_tables() {
        assert!(read(synthetic_elf(Some(&[0xff, 0xfe, 0, 0]))).is_err());
        let mut image = synthetic_elf(Some(&padded("guess")));
        image.truncate(image.len() - 10);
        assert!(read(image).is_err());
    }

    #[test]
    fn unterminated_section_names_do_not_match() {
        assert_eq!(section_name(b"\0abc", 1), None);
        assert_eq!(section_name(b"\0abc\0", 1), Some(b"abc".as_slice()));
        assert_eq!(section_name(b"\0abc\0", 99), None);
    }
}
