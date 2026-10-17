//! Keep logging, child reaping, and launch snapshots alive independently of the UI.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::models::InstalledApp;

pub(crate) const ARGUMENT: &str = "--appbento-launch-monitor";
const RESPONSE_PREFIX: &str = "APPBENTO_LAUNCH_RESULT=";

unsafe extern "C" {
    fn setsid() -> i32;
}

pub fn launch(id: &str, uri_arguments: &[String]) -> Result<InstalledApp, String> {
    let mut command = monitor_command(id, uri_arguments)?;
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Only the async-signal-safe session syscall runs between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start launch monitor: {error}"))?;
    let output = child.stdout.take().ok_or("launch monitor output missing")?;
    std::thread::Builder::new()
        .name("appbento-launch-monitor".into())
        .spawn(move || {
            if let Err(error) = child.wait() {
                eprintln!("could not wait for launch monitor: {error}");
            }
        })
        .map_err(|error| format!("could not watch launch monitor: {error}"))?;
    for line in BufReader::new(output).lines() {
        let line = line.map_err(|error| format!("could not read launch startup: {error}"))?;
        if let Some(response) = line.strip_prefix(RESPONSE_PREFIX) {
            return serde_json::from_str(response)
                .map_err(|error| format!("invalid launch startup response: {error}"))?;
        }
    }
    Err("launch monitor exited before reporting startup".into())
}

fn monitor_command(id: &str, uri_arguments: &[String]) -> Result<Command, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let appimage = std::env::var_os("APPIMAGE");
    // An AppImage needs its own runtime mount after the UI's runtime exits.
    let launcher = crate::lifecycle::desktop::resolve_launcher(
        &executable,
        appimage.as_deref().map(Path::new),
    )?;
    let mut command = Command::new(launcher);
    #[cfg(not(test))]
    command.arg(ARGUMENT).arg(id).args(uri_arguments);
    #[cfg(test)]
    command
        .args([
            "--exact",
            "launch_monitor::tests::monitor_worker",
            "--nocapture",
        ])
        .env(
            "APPBENTO_TEST_LAUNCH_MONITOR",
            serde_json::to_string(&(id, uri_arguments)).map_err(|error| error.to_string())?,
        );
    Ok(command)
}

pub(crate) fn run(id: &str, uri_arguments: &[String]) -> Result<(), String> {
    let mut has_reported_startup = false;
    let result = crate::sandbox::launch_observed(id, uri_arguments, |startup| {
        send_response(startup)?;
        has_reported_startup = true;
        Ok(())
    });
    if let Err(error) = result {
        if !has_reported_startup {
            send_response(Err(&error))?;
        }
        return Err(error);
    }
    Ok(())
}

fn send_response(response: Result<&InstalledApp, &str>) -> Result<(), String> {
    let response = serde_json::to_string(&response).map_err(|error| error.to_string())?;
    let mut output = std::io::stdout().lock();
    writeln!(output, "{RESPONSE_PREFIX}{response}")
        .and_then(|_| output.flush())
        .map_err(|error| format!("could not report launch startup: {error}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn monitor_worker() {
        let Ok(request) = std::env::var("APPBENTO_TEST_LAUNCH_MONITOR") else {
            return;
        };
        let (id, uri_arguments): (String, Vec<String>) = serde_json::from_str(&request).unwrap();
        let result = super::run(&id, &uri_arguments);
        std::process::exit(i32::from(result.is_err()));
    }
}
