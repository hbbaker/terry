//! Runs a child process with captured output and a time limit.

use std::fmt;
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// A process that ended: its exit code (none when a signal ended it) and its output.
pub struct Finished {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum RunError {
    Spawn(io::Error),
    TimedOut(Duration),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Spawn(error) => write!(f, "{error}"),
            RunError::TimedOut(limit) => write!(f, "timed out after {} s", limit.as_secs()),
        }
    }
}

/// Runs `command` with `stdin` as its input, and kills it once `timeout` has passed.
pub fn run_captured(
    command: &mut Command,
    stdin: Option<&str>,
    timeout: Duration,
) -> Result<Finished, RunError> {
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(RunError::Spawn)?;

    let writer = stdin.map(|text| {
        let mut pipe = child.stdin.take().expect("stdin is piped");
        let text = text.to_owned();
        // A child that exits without reading all of it closes the pipe; that is not an error.
        thread::spawn(move || {
            let _ = pipe.write_all(text.as_bytes());
        })
    });
    let stdout = read_in_background(child.stdout.take().expect("stdout is piped"));
    let stderr = read_in_background(child.stderr.take().expect("stderr is piped"));

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut(timeout));
            }
            Err(error) => return Err(RunError::Spawn(error)),
        }
    };
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    Ok(Finished {
        status: status.code(),
        stdout: collect_text(stdout),
        stderr: collect_text(stderr),
    })
}

fn read_in_background(mut pipe: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

fn collect_text(reader: JoinHandle<Vec<u8>>) -> String {
    String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned()
}
