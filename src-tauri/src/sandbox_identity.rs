//! Minimal sandbox account data. Supplementary groups remain kernel-managed.
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd};

const MFD_CLOEXEC: u32 = 1;
const MFD_ALLOW_SEALING: u32 = 2;
const F_ADD_SEALS: i32 = 1033;
const IMMUTABLE_SEALS: i32 = 1 | 2 | 4 | 8;

unsafe extern "C" {
    fn getuid() -> u32;
    fn getgid() -> u32;
    fn memfd_create(name: *const std::ffi::c_char, flags: u32) -> i32;
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

#[derive(Clone, Copy, Debug)]
pub struct Identity {
    pub uid: u32,
    pub gid: u32,
}

impl Identity {
    pub fn current() -> Self {
        // Read the real credentials once at the runtime boundary.
        unsafe {
            Self {
                uid: getuid(),
                gid: getgid(),
            }
        }
    }

    pub(super) fn validate(self) -> Result<(), String> {
        if self.uid == 0 {
            return Err("Restricted launch requires a nonroot UID".into());
        }
        Ok(())
    }

    pub(super) fn contents(self) -> [(String, &'static str); 2] {
        [
            (
                format!("app:x:{}:{}::/home/app:/bin/sh\n", self.uid, self.gid),
                "/etc/passwd",
            ),
            (format!("app:x:{}:\n", self.gid), "/etc/group"),
        ]
    }

    pub(super) fn files(self) -> Result<Vec<(File, &'static str)>, String> {
        self.validate()?;
        self.contents()
            .into_iter()
            .map(|(data, destination)| Ok((sealed_file(data.as_bytes())?, destination)))
            .collect()
    }
}

fn sealed_file(data: &[u8]) -> Result<File, String> {
    let descriptor = unsafe {
        memfd_create(
            c"appbento-account".as_ptr(),
            MFD_CLOEXEC | MFD_ALLOW_SEALING,
        )
    };
    if descriptor == -1 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut file = unsafe { File::from_raw_fd(descriptor) };
    file.write_all(data).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    if unsafe { fcntl(file.as_raw_fd(), F_ADD_SEALS, IMMUTABLE_SEALS) } == -1 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(file)
}
