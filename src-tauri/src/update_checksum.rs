//! Verify downloads against the checksum an update source publishes, without extra dependencies.
use std::io::Read;
use std::path::Path;

use sha1::Sha1;
use sha2::{Digest, Sha512};

use crate::github::ExpectedChecksum;

const READ_BUFFER_BYTES: usize = 64 * 1024;
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) enum ChecksumVerifier {
    Sha1(Sha1, String),
    Sha512(Sha512, String),
}

impl ChecksumVerifier {
    pub(crate) fn new(expected: &ExpectedChecksum) -> Self {
        match expected {
            ExpectedChecksum::Sha1Hex(hex) => Self::Sha1(Sha1::new(), hex.clone()),
            ExpectedChecksum::Sha512Base64(base64) => Self::Sha512(Sha512::new(), base64.clone()),
        }
    }

    pub(crate) fn update(&mut self, chunk: &[u8]) {
        match self {
            Self::Sha1(digest, _) => digest.update(chunk),
            Self::Sha512(digest, _) => digest.update(chunk),
        }
    }

    pub(crate) fn verify(self) -> Result<(), String> {
        let is_match = match self {
            Self::Sha1(digest, expected) => {
                format!("{:x}", digest.finalize()).eq_ignore_ascii_case(expected.trim())
            }
            Self::Sha512(digest, expected) => base64_encode(&digest.finalize()) == expected.trim(),
        };
        if is_match {
            return Ok(());
        }
        Err("the downloaded file does not match the checksum published by the update source".into())
    }
}

pub(crate) fn sha1_hex_of_file(path: &Path) -> Result<String, String> {
    let mut file = crate::lifecycle::open_appimage(path)?;
    let mut digest = Sha1::new();
    let mut buffer = vec![0; READ_BUFFER_BYTES];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&buffer[..count]);
    }
}

pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        let symbol = |shift: u32| char::from(BASE64_ALPHABET[(group >> shift & 0x3f) as usize]);
        encoded.push(symbol(18));
        encoded.push(symbol(12));
        encoded.push(if chunk.len() > 1 { symbol(6) } else { '=' });
        encoded.push(if chunk.len() > 2 { symbol(0) } else { '=' });
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64_with_padding() {
        for (input, expected) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"\xff\xfe\xfd", "//79"),
        ] {
            assert_eq!(base64_encode(input), expected);
        }
    }

    fn verify(expected: ExpectedChecksum, chunks: &[&[u8]]) -> Result<(), String> {
        let mut verifier = ChecksumVerifier::new(&expected);
        for chunk in chunks {
            verifier.update(chunk);
        }
        verifier.verify()
    }

    #[test]
    fn verifies_sha1_hex_ignoring_case() {
        let sha1 = "a9993e364706816aba3e25717850c26c9cd0d89d";
        assert!(verify(ExpectedChecksum::Sha1Hex(sha1.into()), &[b"a", b"bc"]).is_ok());
        assert!(verify(ExpectedChecksum::Sha1Hex(sha1.to_uppercase()), &[b"abc"]).is_ok());
        assert!(verify(ExpectedChecksum::Sha1Hex(sha1.into()), &[b"abd"]).is_err());
    }

    #[test]
    fn verifies_sha512_base64() {
        let expected = base64_encode(&Sha512::digest(b"abc"));
        assert!(verify(
            ExpectedChecksum::Sha512Base64(expected.clone()),
            &[b"ab", b"c"]
        )
        .is_ok());
        assert!(verify(ExpectedChecksum::Sha512Base64(expected), &[b"abd"]).is_err());
        assert!(verify(ExpectedChecksum::Sha512Base64(String::new()), &[b"abc"]).is_err());
    }
}
