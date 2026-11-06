//! Side effects of desktop integration: instance directories, the xdg-dbus-proxy process and cleanup.
use std::collections::BTreeSet;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use super::dbus::{
    app_id, instance_id, proxy_arguments, proxy_rules, resolve_bus_socket, DesktopSession,
    APP_INFO_PLACEHOLDER, BWRAP_INFO_PLACEHOLDER, INSTANCE_ID_PREFIX, INSTANCE_ROOT, PROXY_ROOT,
};
use super::{bwrap_program, LaunchPlan, Runtime};
use crate::models::DesktopIntegration;

const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;
const PROXY_READY_TIMEOUT: Duration = Duration::from_secs(5);
const REFERENCE_FILE: &str = ".ref";
// An instance directory exists briefly before its reference lock does while a launch starts.
const UNLOCKED_INSTANCE_GRACE: Duration = Duration::from_secs(60);
const F_SETLK: i32 = 6;
const F_OFD_GETLK: i32 = 36;
const F_RDLCK: i16 = 0;
const F_WRLCK: i16 = 1;
const F_UNLCK: i16 = 2;
const SEEK_SET: i16 = 0;

#[repr(C)]
struct FileLock {
    kind: i16,
    whence: i16,
    start: i64,
    length: i64,
    process: i32,
}

unsafe extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

/// Builds the session for one launch, failing early when no usable session bus exists.
pub(super) fn resolve_session(
    library_id: &str,
    runtime: &Runtime,
) -> Result<DesktopSession, String> {
    let runtime_directory = runtime
        .directory
        .as_ref()
        .filter(|directory| directory.is_absolute() && directory.is_dir())
        .ok_or("Desktop integration needs XDG_RUNTIME_DIR. Turn off Desktop integration in this app's permissions")?;
    let app_id = app_id(library_id);
    let store = runtime_directory.join("doc/by-app").join(&app_id);
    Ok(DesktopSession {
        bus_socket: resolve_bus_socket(
            runtime.dbus_session_bus_address.as_deref(),
            runtime_directory,
        )?,
        instance_id: instance_id(random_u64()?),
        document_store: store.exists().then_some(store),
        runtime_directory: runtime_directory.clone(),
        app_id,
    })
}

fn random_u64() -> Result<u64, String> {
    let mut bytes = [0; 8];
    File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| format!("could not generate an instance ID: {error}"))?;
    Ok(u64::from_le_bytes(bytes))
}

/// The proxy and instance directories of one running sandbox; dropping it tears them down.
pub(super) struct DesktopProcess {
    session: DesktopSession,
    proxy: Option<Child>,
    // Closing the read end tells the proxy to exit.
    sync: Option<std::io::PipeReader>,
    // Held for its read lock so Flatpak's instance cleanup treats the directory as live.
    reference: Option<File>,
}

impl DesktopProcess {
    pub(super) fn start(
        session: DesktopSession,
        integration: &DesktopIntegration,
        proxy_program: &str,
    ) -> Result<Self, String> {
        let mut process = Self {
            session,
            proxy: None,
            sync: None,
            reference: None,
        };
        process.create_directories()?;
        process.start_proxy(proxy_program, &proxy_rules(integration))?;
        Ok(process)
    }

    /// Opens the descriptors the app's Bubblewrap needs and substitutes them into the plan.
    pub(super) fn attach(&self, plan: &mut LaunchPlan) -> Result<Vec<File>, String> {
        let directory = self.session.instance_directory();
        let info = File::open(directory.join("info")).map_err(describe("open sandbox info"))?;
        let bwrap_info = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(PRIVATE_FILE_MODE)
            .open(directory.join("bwrapinfo.json"))
            .map_err(describe("create bwrapinfo.json"))?;
        let replacements = [
            (APP_INFO_PLACEHOLDER, info.as_raw_fd()),
            (BWRAP_INFO_PLACEHOLDER, bwrap_info.as_raw_fd()),
        ];
        for (placeholder, descriptor) in replacements {
            let argument = plan
                .arguments
                .iter_mut()
                .find(|argument| *argument == placeholder)
                .ok_or("desktop integration arguments missing from launch plan")?;
            *argument = descriptor.to_string();
        }
        Ok(vec![info, bwrap_info])
    }

    fn create_directories(&mut self) -> Result<(), String> {
        let instance = self.session.instance_directory();
        let proxy = self.session.proxy_directory();
        create_private_directory(instance.parent().ok_or("invalid instance directory")?, true)?;
        create_private_directory(&instance, false)?;
        write_private_file(&instance.join("info"), &self.session.keyfile())?;
        let reference = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(PRIVATE_FILE_MODE)
            .open(instance.join(REFERENCE_FILE))
            .map_err(describe("create instance reference"))?;
        lock_shared(&reference).map_err(describe("lock instance reference"))?;
        self.reference = Some(reference);
        // Created only once the lock is held, so the startup sweep never mistakes it for stale.
        create_private_directory(proxy.parent().ok_or("invalid proxy directory")?, true)?;
        create_private_directory(&proxy, false)?;
        Ok(())
    }

    fn start_proxy(&mut self, program: &str, rules: &[String]) -> Result<(), String> {
        let instance = self.session.instance_directory();
        let info = File::open(instance.join("info")).map_err(describe("open sandbox info"))?;
        let (reader, writer) = std::io::pipe().map_err(describe("create proxy pipe"))?;
        let log = File::create(instance.join("proxy.log")).map_err(describe("create proxy log"))?;
        let descriptors = [info.as_raw_fd(), writer.as_raw_fd()];
        let mut command = Command::new(bwrap_program());
        command
            .args(proxy_arguments(
                &self.session,
                program,
                rules,
                descriptors[0],
                descriptors[1],
            )?)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log);
        super::execution::inherit_only_descriptors(&mut command, descriptors.to_vec());
        self.proxy = Some(
            command
                .spawn()
                .map_err(|error| format!("could not start xdg-dbus-proxy: {error}"))?,
        );
        // The proxy now owns its copies; ours must close for EOF to signal a dead proxy.
        drop(command);
        drop(writer);
        drop(info);
        self.sync = Some(self.wait_until_ready(reader)?);
        Ok(())
    }

    fn wait_until_ready(&self, reader: std::io::PipeReader) -> Result<std::io::PipeReader, String> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("appbento-dbus-proxy-ready".into())
            .spawn(move || {
                let mut reader = reader;
                let result = reader.read(&mut [0; 1]);
                // The receiver is gone only after a timeout, when nobody needs the answer.
                let _ = sender.send((result, reader));
            })
            .map_err(describe("watch xdg-dbus-proxy"))?;
        match receiver.recv_timeout(PROXY_READY_TIMEOUT) {
            Ok((Ok(1), reader)) => Ok(reader),
            Ok(_) => Err(self.proxy_failure("exited before it was ready")),
            Err(_) => Err(self.proxy_failure("did not become ready in time")),
        }
    }

    fn proxy_failure(&self, reason: &str) -> String {
        let log = fs::read_to_string(self.session.instance_directory().join("proxy.log"))
            .unwrap_or_default();
        let detail = log.trim();
        format!(
            "xdg-dbus-proxy {reason}{}. Turn off Desktop integration in this app's permissions to launch without it",
            if detail.is_empty() { String::new() } else { format!(": {detail}") }
        )
    }
}

impl Drop for DesktopProcess {
    fn drop(&mut self) {
        drop(self.sync.take());
        if let Some(mut proxy) = self.proxy.take() {
            if let Err(error) = proxy.kill().and_then(|_| proxy.wait().map(drop)) {
                eprintln!("cannot stop xdg-dbus-proxy: {error}");
            }
        }
        for directory in [
            self.session.instance_directory(),
            self.session.proxy_directory(),
        ] {
            if let Err(error) = fs::remove_dir_all(&directory) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("cannot remove {}: {error}", directory.display());
                }
            }
        }
        // The lock goes away with the descriptor, after the directory is gone.
        drop(self.reference.take());
    }
}

fn create_private_directory(path: &Path, recursive: bool) -> Result<(), String> {
    DirBuilder::new()
        .recursive(recursive)
        .mode(PRIVATE_DIRECTORY_MODE)
        .create(path)
        .map_err(describe(&format!("create {}", path.display())))
}

fn write_private_file(path: &Path, contents: &str) -> Result<(), String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PRIVATE_FILE_MODE)
        .open(path)
        .and_then(|mut file| file.write_all(contents.as_bytes()))
        .map_err(describe(&format!("write {}", path.display())))
}

pub(super) fn lock_shared(file: &File) -> std::io::Result<()> {
    let lock = FileLock {
        kind: F_RDLCK,
        whence: SEEK_SET,
        start: 0,
        length: 0,
        process: 0,
    };
    // F_SETLK takes a pointer to a flock structure with this layout on 64-bit Linux.
    if unsafe { fcntl(file.as_raw_fd(), F_SETLK, &lock as *const FileLock) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Removes proxy and instance directories that launch monitors killed before cleanup left behind.
pub(crate) fn remove_stale_directories(runtime_directory: &Path) -> Result<(), String> {
    let instances = runtime_directory.join(INSTANCE_ROOT);
    let proxies = runtime_directory.join(PROXY_ROOT);
    let mut instance_ids = owned_instance_ids(&instances)?;
    instance_ids.extend(owned_instance_ids(&proxies)?);
    for id in instance_ids {
        if is_instance_live(&instances.join(&id))? {
            continue;
        }
        for directory in [instances.join(&id), proxies.join(&id)] {
            remove_if_present(&directory)?;
        }
    }
    Ok(())
}

/// Lists AppBento's directories only; other Flatpak instances are never touched.
fn owned_instance_ids(directory: &Path) -> Result<BTreeSet<String>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(describe(&format!("read {}", directory.display()))(error)),
    };
    let mut ids = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(describe(&format!("read {}", directory.display())))?;
        let is_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        let name = entry.file_name().into_string().unwrap_or_default();
        if is_directory && is_owned_instance_id(&name) {
            ids.insert(name);
        }
    }
    Ok(ids)
}

fn is_owned_instance_id(name: &str) -> bool {
    name.strip_prefix(INSTANCE_ID_PREFIX).is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix
                .chars()
                .all(|character| character.is_ascii_hexdigit())
    })
}

fn is_instance_live(instance: &Path) -> Result<bool, String> {
    let reference = match File::open(instance.join(REFERENCE_FILE)) {
        Ok(reference) => reference,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(is_recent(instance)),
        Err(error) => return Err(describe("open instance reference")(error)),
    };
    is_locked(&reference).map_err(describe("check instance reference"))
}

fn is_recent(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < UNLOCKED_INSTANCE_GRACE)
}

/// Open file description locks conflict with every other lock, including this process's own.
fn is_locked(file: &File) -> std::io::Result<bool> {
    let mut lock = FileLock {
        kind: F_WRLCK,
        whence: SEEK_SET,
        start: 0,
        length: 0,
        process: 0,
    };
    if unsafe { fcntl(file.as_raw_fd(), F_OFD_GETLK, &mut lock as *mut FileLock) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(lock.kind != F_UNLCK)
}

fn remove_if_present(directory: &Path) -> Result<(), String> {
    match fs::remove_dir_all(directory) {
        Err(error) if error.kind() != ErrorKind::NotFound => {
            Err(describe(&format!("remove {}", directory.display()))(error))
        }
        _ => Ok(()),
    }
}

fn describe(action: impl Into<String>) -> impl Fn(std::io::Error) -> String {
    let action = action.into();
    move |error| format!("could not {action}: {error}")
}
