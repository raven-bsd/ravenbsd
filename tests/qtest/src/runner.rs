// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process execution with serial capture, ANSI stripping and a hard timeout.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use regex::Regex;

/// A process to execute and the time it may run.
pub(crate) struct RunSpec {
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
}

/// Captured output and exit status.
pub(crate) struct RunOutput {
    pub lines: Vec<String>,
    pub status: Option<i32>,
    pub timed_out: bool,
}

/// Run `spec`, streaming stdout line by line and killing the process group on timeout.
pub(crate) fn run(spec: &RunSpec) -> Result<RunOutput> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("starting `{}`", spec.program))?;
    let stdout = child
        .stdout
        .take()
        .context("child process has no captured stdout")?;
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) => break,
                Ok(_) => {
                    if sender.send(clean_line(&buffer)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender.send(format!("<serial read error: {error}>"));
                    break;
                }
            }
        }
    });

    let started = Instant::now();
    let mut lines = Vec::new();
    let mut timed_out = false;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() >= spec.timeout {
            timed_out = true;
            kill_tree(&mut child);
            break;
        }
        let remaining = spec.timeout.saturating_sub(started.elapsed());
        let slice = remaining.min(Duration::from_millis(10));
        match receiver.recv_timeout(slice) {
            Ok(line) => lines.push(line),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                // stdout closed but the process is still alive; poll until it exits.
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
    let status = child.wait()?.code();
    let _ = reader.join();
    lines.extend(receiver.try_iter());

    Ok(RunOutput {
        lines,
        status,
        timed_out,
    })
}

fn clean_line(bytes: &[u8]) -> String {
    let mut line = String::from_utf8_lossy(bytes).into_owned();
    if line.ends_with('\n') {
        line.pop();
    }
    if line.ends_with('\r') {
        line.pop();
    }
    strip_ansi(&line)
}

fn strip_ansi(line: &str) -> String {
    static ANSI: OnceLock<Option<Regex>> = OnceLock::new();
    let regex = ANSI.get_or_init(|| Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").ok());
    match regex {
        Some(regex) => regex.replace_all(line, "").into_owned(),
        None => line.to_string(),
    }
}

#[cfg(unix)]
fn kill_tree(child: &mut Child) {
    let pid = child.id();
    let _ = Command::new("kill")
        .args(["-s", "KILL", "--", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill_tree(child: &mut Child) {
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(script: &str, timeout: Duration) -> RunSpec {
        RunSpec {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            timeout,
        }
    }

    #[test]
    fn strips_crlf_and_ansi_escapes() {
        let output = run(&spec(
            "printf 'hello\\r\\n\\033[31mred\\033[0m\\n'",
            Duration::from_secs(5),
        ))
        .unwrap();
        assert_eq!(output.lines, vec!["hello", "red"]);
        assert_eq!(output.status, Some(0));
    }

    #[test]
    fn flushes_a_partial_last_line() {
        let output = run(&spec("printf 'tail'", Duration::from_secs(5))).unwrap();
        assert_eq!(output.lines, vec!["tail"]);
    }

    #[test]
    fn binary_garbage_does_not_panic() {
        let output = run(&spec("printf '\\377\\376\\n'", Duration::from_secs(5))).unwrap();
        assert_eq!(output.lines.len(), 1);
        assert!(output.lines[0].contains('\u{fffd}'));
    }

    #[test]
    fn very_long_line_does_not_hang() {
        let output = run(&spec(
            "head -c 1048576 /dev/zero | tr '\\0' x; printf '\\n'",
            Duration::from_secs(10),
        ))
        .unwrap();
        assert_eq!(output.lines.len(), 1);
        assert_eq!(output.lines[0].len(), 1_048_576);
    }

    #[test]
    fn exit_status_is_captured() {
        let output = run(&spec("exit 33", Duration::from_secs(5))).unwrap();
        assert_eq!(output.status, Some(33));
        assert!(!output.timed_out);
    }

    #[test]
    fn timeout_kills_a_sleeping_process() {
        let started = Instant::now();
        let output = run(&spec("sleep 5", Duration::from_millis(500))).unwrap();
        assert!(output.timed_out);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(output.status, None);
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_the_whole_process_group() {
        let temp = crate::testutil::TempDir::new("runner-group");
        let pid_file = temp.path().join("child.pid");
        let script = format!("sleep 30 & echo $! > {}; wait", pid_file.display());
        let output = run(&spec(&script, Duration::from_millis(500))).unwrap();
        assert!(output.timed_out);

        let bytes = std::fs::read(&pid_file).expect("pid file written before the timeout");
        let pid: u32 = String::from_utf8_lossy(&bytes).trim().parse().unwrap();
        let mut alive = true;
        for _ in 0..20 {
            let status = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            if !status.success() {
                alive = false;
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            !alive,
            "child process {pid} survived the process-group kill"
        );
    }

    #[cfg(unix)]
    #[test]
    fn timeout_leaves_no_process_matching_the_runner() {
        let marker = format!("raven-qtest-orphan-{}", std::process::id());
        let script = format!("# {marker}\nsleep 30 & wait");
        let output = run(&spec(&script, Duration::from_millis(500))).unwrap();
        assert!(output.timed_out);
        thread::sleep(Duration::from_millis(100));
        let pgrep = Command::new("pgrep")
            .arg("-f")
            .arg(&marker)
            .output()
            .expect("pgrep runs");
        assert!(
            !pgrep.status.success(),
            "process survived the timeout: {}",
            String::from_utf8_lossy(&pgrep.stdout)
        );
    }
}
