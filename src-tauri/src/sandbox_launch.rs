use super::desktop_process::{resolve_session, DesktopProcess};
use super::{
    open_app, plan, private_home_path, status, validate_grant, xdg_dbus_proxy_program, LaunchPlan,
    Runtime,
};
use crate::lifecycle::app_digest;
use crate::models::{DesktopIntegration, InstalledApp, LaunchMode};
use crate::persistence;
use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};

const O_PATH: i32 = 0x200000;
const O_NOFOLLOW: i32 = 0x20000;
const F_SETFD: i32 = 2;
const STARTUP_CHECK_DELAY: std::time::Duration = std::time::Duration::from_millis(150);
unsafe extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}

pub(crate) fn launch_observed(
    id: &str,
    uri_arguments: &[String],
    report_startup: impl FnOnce(Result<&InstalledApp, &str>) -> Result<(), String>,
) -> Result<InstalledApp, String> {
    let recorder = crate::launch_logs::Recorder::start(id)?;
    let result = launch_recorded(id, uri_arguments, &recorder, report_startup);
    if let Err(error) = &result {
        recorder.record_error(error);
    }
    result
}

fn launch_recorded(
    id: &str,
    uri_arguments: &[String],
    recorder: &crate::launch_logs::Recorder,
    report_startup: impl FnOnce(Result<&InstalledApp, &str>) -> Result<(), String>,
) -> Result<InstalledApp, String> {
    let app = crate::store::load_all()?
        .remove(id)
        .ok_or("app not found")?;
    if app.launch_config.is_some()
        && app.reviewed_digest.as_deref()
            != Some(app_digest(Path::new(&app.file_path), app.kind)?.as_str())
    {
        crate::store::mutate_library(|library| {
            let app = library.get_mut(id).ok_or("app not found")?;
            app.reviewed_digest = None;
            Ok(())
        })?;
        return Err("App changed since review. Review launch permissions again".into());
    }
    let policy_directory = persistence::data_path("library.json")?
        .parent()
        .ok_or("invalid policy directory")?
        .to_owned();
    let desktop_integration = sandboxed_desktop_integration(&app);
    let mut runtime = Runtime::current();
    if desktop_integration.is_some() {
        runtime.desktop_session = Some(resolve_session(&app.id, &runtime)?);
    }
    let mut plan = plan(&app, uri_arguments, &policy_directory, &runtime)?;
    if app
        .launch_config
        .as_ref()
        .is_some_and(|config| config.mode == LaunchMode::Bubblewrap)
    {
        let status = status();
        if !status.available {
            return Err(status
                .error
                .unwrap_or_else(|| "Bubblewrap unavailable".into()));
        }
        if desktop_integration.is_some() && !status.desktop_integration_available {
            return Err(status
                .desktop_integration_error
                .unwrap_or_else(|| "Desktop integration unavailable".into()));
        }
        if app
            .launch_config
            .as_ref()
            .is_some_and(|config| config.permissions.persistent_data)
        {
            prepare_private_home(&policy_directory, id)?;
        }
    }
    let mut pinned = pin_launch_paths_in_place(&mut plan, &app, &policy_directory)?;
    // Declared after `pinned` and dropped first: the proxy and instance files outlive the app.
    let _desktop = match (runtime.desktop_session.take(), desktop_integration) {
        (Some(session), Some(integration)) => Some(start_desktop(
            session,
            &integration,
            &mut plan,
            &mut pinned,
        )?),
        _ => None,
    };
    let mut command = prepare_command(&plan);
    inherit_descriptors(&mut command, &pinned);
    let capture = recorder.capture(&mut command)?;
    let startup = spawn_checked(&mut command);
    drop(command);
    let result = match startup {
        Ok(mut child) => {
            let launched = crate::store::touch_launch(id);
            let reported = report_startup(launched.as_ref().map_err(String::as_str));
            let exit = wait_for_app(&mut child);
            exit.and(reported).and(launched)
        }
        Err(error) => Err(error),
    };
    capture.join().map_err(|_| "launch log capture panicked")?;
    result
}

fn sandboxed_desktop_integration(app: &InstalledApp) -> Option<DesktopIntegration> {
    app.launch_config
        .as_ref()
        .filter(|config| config.mode == LaunchMode::Bubblewrap)
        .map(|config| config.permissions.desktop_integration.clone())
        .filter(DesktopIntegration::is_enabled)
}

fn start_desktop(
    session: super::DesktopSession,
    integration: &DesktopIntegration,
    plan: &mut LaunchPlan,
    pinned: &mut PinnedLaunch,
) -> Result<DesktopProcess, String> {
    let program = xdg_dbus_proxy_program().ok_or("xdg-dbus-proxy is not installed")?;
    let process = DesktopProcess::start(session, integration, program)?;
    pinned.files.extend(process.attach(plan)?);
    Ok(process)
}

fn spawn_checked(command: &mut Command) -> Result<Child, String> {
    let mut child = command
        .spawn()
        .map_err(|error| format!("launch failed: {error}"))?;
    std::thread::sleep(STARTUP_CHECK_DELAY);
    if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
        if !status.success() {
            return Err(format!("App launch failed: {status}"));
        }
    }
    Ok(child)
}

fn wait_for_app(child: &mut Child) -> Result<(), String> {
    let status = child
        .wait()
        .map_err(|error| format!("could not wait for app: {error}"))?;
    if !status.success() {
        return Err(format!("App exited with {status}"));
    }
    Ok(())
}

fn prepare_private_home(policy_directory: &Path, id: &str) -> Result<(), String> {
    let home = private_home_path(policy_directory, id);
    let parent = home.parent().ok_or("invalid private home")?;
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("private data parent must be a non-symlink directory".into())
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(parent).map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    if !home.try_exists().map_err(|error| error.to_string())? {
        fs::create_dir(&home).map_err(|error| error.to_string())?;
    }
    if fs::symlink_metadata(&home)
        .map_err(|error| error.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("private application home must not be a symlink".into());
    }
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())
}

pub(super) fn pin_launch_paths_in_place(
    plan: &mut LaunchPlan,
    app: &InstalledApp,
    policy_directory: &Path,
) -> Result<PinnedLaunch, String> {
    if !plan.clear_environment {
        return Ok(PinnedLaunch {
            files: Vec::new(),
            snapshot_path: None,
        });
    }
    let (mut image, snapshot_path) = snapshot_app(app, policy_directory)?;
    let mut pinned = PinnedLaunch {
        files: Vec::new(),
        snapshot_path: Some(snapshot_path),
    };
    if app.reviewed_digest.as_deref() != Some(super::digest_file(&mut image)?.as_str()) {
        return Err("App changed since review. Review launch permissions again".into());
    }
    let image_descriptor = image.as_raw_fd();
    pinned.files.push(image);
    for grant in &app
        .launch_config
        .as_ref()
        .ok_or("launch configuration missing")?
        .permissions
        .paths
    {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(O_PATH | O_NOFOLLOW)
            .open(&grant.path)
            .map_err(|error| error.to_string())?;
        if !file.metadata().map_err(|error| error.to_string())?.is_dir() {
            return Err(
                "folder grant changed or became a symlink; review permissions again".into(),
            );
        }
        let pinned_path = fs::canonicalize(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|error| error.to_string())?;
        validate_grant(&pinned_path, policy_directory)?;
        let mount_indices = plan
            .arguments
            .windows(3)
            .enumerate()
            .filter(|(_, arguments)| {
                (arguments[0] == "--bind" || arguments[0] == "--ro-bind")
                    && arguments[1] == grant.path
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if mount_indices.len() != 1 || pinned_path != Path::new(&grant.path) {
            return Err("folder grant changed since review; review permissions again".into());
        }
        for arguments in mount_indices {
            plan.arguments[arguments] = if plan.arguments[arguments] == "--bind" {
                "--bind-fd"
            } else {
                "--ro-bind-fd"
            }
            .into();
            plan.arguments[arguments + 1] = file.as_raw_fd().to_string();
        }
        pinned.files.push(file);
    }
    let image_index = plan
        .arguments
        .windows(3)
        .position(|arguments| {
            arguments[0] == "--ro-bind"
                && Some(arguments[2].as_str()) == plan.application_destination.as_deref()
                && arguments[1] == app.file_path
        })
        .ok_or("application mount missing")?;
    plan.arguments[image_index] = "--ro-bind-fd".into();
    plan.arguments[image_index + 1] = image_descriptor.to_string();
    if app
        .launch_config
        .as_ref()
        .is_some_and(|config| config.permissions.persistent_data)
    {
        let home = private_home_path(policy_directory, &app.id);
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(O_PATH | O_NOFOLLOW)
            .open(&home)
            .map_err(|error| error.to_string())?;
        let index = plan
            .arguments
            .windows(3)
            .position(|arguments| arguments[0] == "--bind" && arguments[2] == "/home/app")
            .ok_or("private home mount missing")?;
        plan.arguments[index] = "--bind-fd".into();
        plan.arguments[index + 1] = file.as_raw_fd().to_string();
        pinned.files.push(file);
    }
    Ok(pinned)
}

pub(super) fn inherit_descriptors(command: &mut Command, pinned: &PinnedLaunch) {
    let descriptors = pinned
        .files
        .iter()
        .map(AsRawFd::as_raw_fd)
        .collect::<Vec<_>>();
    // Only descriptor flag changes run in the forked child before exec.
    unsafe {
        command.pre_exec(move || {
            for descriptor in &descriptors {
                if fcntl(*descriptor, F_SETFD, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
}

fn snapshot_app(
    app: &InstalledApp,
    policy_directory: &Path,
) -> Result<(File, std::path::PathBuf), String> {
    let mut original = open_app(Path::new(&app.file_path), app.kind)?;
    let (temporary, mut staged) = persistence::create_temporary(policy_directory)?;
    let preparation = (|| {
        std::io::copy(&mut original, &mut staged).map_err(|error| error.to_string())?;
        staged
            .set_permissions(fs::Permissions::from_mode(0o500))
            .map_err(|error| error.to_string())?;
        drop(staged);
        open_app(&temporary, app.kind)
    })();
    match preparation {
        Ok(file) => Ok((file, temporary)),
        Err(error) => {
            fs::remove_file(&temporary)
                .map_err(|cleanup| format!("{error}; cannot clean launch snapshot: {cleanup}"))?;
            Err(error)
        }
    }
}

pub(super) struct PinnedLaunch {
    files: Vec<File>,
    snapshot_path: Option<std::path::PathBuf>,
}

impl Drop for PinnedLaunch {
    fn drop(&mut self) {
        let Some(path) = &self.snapshot_path else {
            return;
        };
        if let Err(error) = fs::remove_file(path) {
            eprintln!("cannot remove launch snapshot: {error}");
        }
    }
}

pub(super) fn prepare_command(plan: &LaunchPlan) -> Command {
    let mut command = Command::new(&plan.program);
    command
        .args(&plan.arguments)
        .stdin(std::process::Stdio::inherit());
    if plan.clear_environment {
        command.env_clear().stdin(std::process::Stdio::null());
    }
    command.envs(&plan.environment);
    command
}
