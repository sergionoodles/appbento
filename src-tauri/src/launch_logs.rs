//! Persist only the newest launch, including output received while it is running.
use std::collections::VecDeque;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::persistence;

const MAX_LINES: usize = 5_000;
// Bound memory and disk usage even for an application that never writes a newline.
const MAX_LINE_BYTES: usize = 16_384;
const READ_BUFFER_BYTES: usize = 8_192;

#[derive(Default, Serialize, Deserialize)]
struct SavedLog {
    session: String,
    started_at: String,
    lines: VecDeque<String>,
    pending: Vec<u8>,
}

#[derive(Serialize)]
pub struct LaunchLog {
    started_at: String,
    text: String,
}

#[derive(Clone)]
pub struct Recorder {
    path: PathBuf,
    session: String,
}

fn log_path(id: &str) -> Result<PathBuf, String> {
    persistence::data_path(&format!(
        "launch-logs/{:x}.json",
        Sha256::digest(id.as_bytes())
    ))
}

pub fn read(id: &str) -> Result<Option<LaunchLog>, String> {
    read_path(&log_path(id)?)
}

fn read_path(path: &std::path::Path) -> Result<Option<LaunchLog>, String> {
    let saved: SavedLog = persistence::load(path)?;
    if saved.session.is_empty() {
        return Ok(None);
    }
    let mut lines = saved.lines.into_iter().collect::<Vec<_>>();
    if !saved.pending.is_empty() {
        lines.push(String::from_utf8_lossy(&saved.pending).into_owned());
    }
    Ok(Some(LaunchLog {
        started_at: saved.started_at,
        text: lines.join("\n").trim_start_matches(['\r', '\n']).to_owned(),
    }))
}

impl Recorder {
    pub fn start(id: &str) -> Result<Self, String> {
        Self::start_at(log_path(id)?)
    }

    fn start_at(path: PathBuf) -> Result<Self, String> {
        let now = chrono::Utc::now();
        let session = format!(
            "{}-{}",
            std::process::id(),
            now.timestamp_nanos_opt().unwrap_or_default()
        );
        persistence::mutate(&path, |saved: &mut SavedLog| {
            *saved = SavedLog {
                session: session.clone(),
                started_at: now.to_rfc3339(),
                ..Default::default()
            };
            Ok(())
        })?;
        Ok(Self { path, session })
    }

    pub fn append(&self, bytes: &[u8]) -> Result<(), String> {
        persistence::with_repository(
            &self.path,
            |repository: &mut persistence::Repository<SavedLog>| {
                if repository.state.session != self.session {
                    return Ok(());
                }
                append_bytes_in_place(&mut repository.state, bytes);
                repository.commit().map_err(|error| error.message)
            },
        )
    }

    pub fn record_error(&self, error: &str) {
        if let Err(save_error) = self.append(format!("\n[AppBento] {error}\n").as_bytes()) {
            eprintln!("cannot save launch error: {save_error}");
        }
    }

    pub fn capture(&self, command: &mut Command) -> Result<std::thread::JoinHandle<()>, String> {
        let (mut reader, writer) = UnixStream::pair().map_err(|error| error.to_string())?;
        let stderr = writer.try_clone().map_err(|error| error.to_string())?;
        command.stdout(Stdio::from(OwnedFd::from(writer)));
        command.stderr(Stdio::from(OwnedFd::from(stderr)));
        let recorder = self.clone();
        let capture = std::thread::Builder::new()
            .name("appbento-launch-log".into())
            .spawn(move || {
                let mut buffer = [0; READ_BUFFER_BYTES];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => {
                            if let Err(error) = recorder.append(&buffer[..count]) {
                                eprintln!("cannot save launch output: {error}");
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(error) => {
                            recorder.record_error(&format!("cannot read launch output: {error}"));
                            break;
                        }
                    }
                }
            })
            .map_err(|error| format!("cannot start log capture: {error}"))?;
        Ok(capture)
    }
}

fn append_bytes_in_place(saved: &mut SavedLog, bytes: &[u8]) {
    for byte in bytes {
        if *byte != b'\n' {
            saved.pending.push(*byte);
        }
        if *byte == b'\n' || saved.pending.len() == MAX_LINE_BYTES {
            saved.lines.push_back(
                String::from_utf8_lossy(&saved.pending)
                    .trim_end_matches('\r')
                    .to_owned(),
            );
            saved.pending.clear();
        }
        let line_count = saved.lines.len() + usize::from(!saved.pending.is_empty());
        if line_count > MAX_LINES {
            saved.lines.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::tests::TemporaryDirectory;

    #[test]
    fn reads_launch_errors_without_a_leading_blank_line() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("log.json");
        let recorder = Recorder::start_at(path.clone()).unwrap();
        recorder.record_error("launch failed");
        assert_eq!(
            read_path(&path).unwrap().unwrap().text,
            "[AppBento] launch failed"
        );
    }

    #[test]
    fn removes_leading_blank_lines_but_preserves_indentation_and_internal_spacing() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("log.json");
        let recorder = Recorder::start_at(path.clone()).unwrap();
        recorder
            .append(b"\r\n\n  diagnostic\n\n  details\n")
            .unwrap();
        assert_eq!(
            read_path(&path).unwrap().unwrap().text,
            "  diagnostic\n\n  details"
        );
    }

    #[test]
    fn retains_latest_lines_and_partial_output_on_disk() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("log.json");
        assert!(read_path(&path).unwrap().is_none());
        let recorder = Recorder::start_at(path.clone()).unwrap();
        let output = (0..5_100)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        recorder.append(output.as_bytes()).unwrap();
        recorder.append(b"partial").unwrap();
        let log = read_path(&path).unwrap().unwrap();
        assert_eq!(log.text.lines().count(), MAX_LINES);
        assert!(log.text.starts_with("line 101\n"));
        assert!(log.text.ends_with("partial"));
        assert!(!log.started_at.is_empty());
    }

    #[test]
    fn new_launch_replaces_output_and_ignores_old_process() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("log.json");
        let old = Recorder::start_at(path.clone()).unwrap();
        old.append(b"old launch\n").unwrap();
        let new = Recorder::start_at(path.clone()).unwrap();
        new.append(b"new launch\n").unwrap();
        old.append(b"late old output\n").unwrap();
        assert_eq!(read_path(&path).unwrap().unwrap().text, "new launch");
    }

    #[test]
    fn captures_stdout_stderr_and_unterminated_errors() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("log.json");
        let recorder = Recorder::start_at(path.clone()).unwrap();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf 'stdout\\n'; printf 'stderr' >&2; exit 7"]);
        let capture = recorder.capture(&mut command).unwrap();
        let mut child = command.spawn().unwrap();
        drop(command);
        assert!(!child.wait().unwrap().success());
        capture.join().unwrap();
        assert_eq!(read_path(&path).unwrap().unwrap().text, "stdout\nstderr");
    }

    #[test]
    fn bounds_output_without_newlines_and_preserves_split_utf8() {
        let mut saved = SavedLog::default();
        append_bytes_in_place(&mut saved, &[b'x'; MAX_LINE_BYTES + 1]);
        assert_eq!(saved.lines[0].len(), MAX_LINE_BYTES);
        assert_eq!(saved.pending, b"x");
        let mut saved = SavedLog::default();
        append_bytes_in_place(&mut saved, &[0xc3]);
        append_bytes_in_place(&mut saved, &[0xa8, b'\n']);
        assert_eq!(saved.lines[0], "è");
    }

    #[test]
    fn saves_launch_preparation_errors() {
        let directory = TemporaryDirectory::new();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "launch_logs::tests::launch_failure_worker",
                "--nocapture",
            ])
            .env("APPBENTO_TEST_LOG_DIRECTORY", &directory.0)
            .env("XDG_DATA_HOME", &directory.0)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    #[test]
    fn launch_failure_worker() {
        if std::env::var_os("APPBENTO_TEST_LOG_DIRECTORY").is_none() {
            return;
        }
        let id = "missing-app";
        let error = crate::sandbox::launch(id, &[]).unwrap_err();
        assert_eq!(error, "app not found");
        assert!(read(id)
            .unwrap()
            .unwrap()
            .text
            .contains("[AppBento] app not found"));
    }
}
