// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process execution with serial capture, scripted dialogue, ANSI stripping and a hard timeout.

use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use regex::Regex;

use crate::dialogue::Dialogue;

/// A process to execute and the time it may run.
pub(crate) struct RunSpec {
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
}

/// Why a `[[step]]` did not finish.
#[derive(Debug)]
pub(crate) struct StepFailure {
    /// Zero-based step index in file order.
    pub index: usize,
    /// The wait regex, or the send text, of the pending step.
    pub pattern: String,
    /// What happened instead.
    pub reason: String,
    /// The last serial lines (at most 20).
    pub lines: Vec<String>,
}

/// Captured output and exit status.
pub(crate) struct RunOutput {
    pub lines: Vec<String>,
    pub status: Option<i32>,
    pub timed_out: bool,
    /// Set when the scripted dialogue did not finish.
    pub step_failure: Option<StepFailure>,
}

/// Run `spec`, streaming stdout line by line and killing the process group on timeout.
pub(crate) fn run(spec: &RunSpec) -> Result<RunOutput> {
    run_inner(spec, None)
}

/// Run `spec` with a scripted dialogue on the guest's serial input.
///
/// The child's stdin is a pipe: every `send` is written there, which is the QEMU serial input
/// under `-serial stdio` / `-nographic`. A step that cannot finish (its timeout, the case
/// timeout, or the process exiting first) is reported in [`RunOutput::step_failure`].
pub(crate) fn run_dialogue(spec: &RunSpec, dialogue: &mut Dialogue) -> Result<RunOutput> {
    run_inner(spec, Some(dialogue))
}

fn run_inner(spec: &RunSpec, mut dialogue: Option<&mut Dialogue>) -> Result<RunOutput> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if dialogue.is_some() {
        command.stdin(Stdio::piped());
    }
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
    let mut stdin = child.stdin.take();
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut stdout = stdout;
        let mut buffer = [0u8; 65_536];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(length) => {
                    if sender.send(buffer[..length].to_vec()).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender.send(format!("<serial read error: {error}>\n").into_bytes());
                    break;
                }
            }
        }
    });

    let started = Instant::now();
    let mut lines = Vec::new();
    let mut raw = Vec::new();
    let mut timed_out = false;
    let mut step_failure: Option<StepFailure> = None;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        let now = Instant::now();
        if now.duration_since(started) >= spec.timeout {
            timed_out = true;
            kill_tree(&mut child);
            break;
        }
        if let Some(active) = dialogue.as_deref_mut() {
            if let Some(deadline) = active.wait_deadline()
                && now >= deadline
                && let Some((index, pattern, timeout)) = active.pending_step()
            {
                step_failure = Some(StepFailure {
                    index,
                    pattern: pattern.to_string(),
                    reason: format!("no matching serial line within {} s", timeout.as_secs()),
                    lines: tail(&lines, 20),
                });
                kill_tree(&mut child);
                break;
            }
            if let Some(payload) = active.poll_send(now) {
                write_stdin(&mut stdin, &payload);
            }
        }
        let remaining = spec.timeout.saturating_sub(started.elapsed());
        let slice = remaining.min(Duration::from_millis(10));
        match receiver.recv_timeout(slice) {
            Ok(chunk) => consume(&chunk, &mut raw, &mut lines, &mut dialogue),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                // stdout closed but the process is still alive; poll until it exits.
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
    let status = child.wait()?.code();
    let _ = reader.join();
    for chunk in receiver.try_iter() {
        consume(&chunk, &mut raw, &mut lines, &mut dialogue);
    }
    if !raw.is_empty() {
        let line = clean_bytes(&raw);
        raw.clear();
        if let Some(active) = dialogue.as_deref_mut() {
            active.line(&line, Instant::now());
        }
        lines.push(line);
    }
    if let Some(active) = dialogue
        && step_failure.is_none()
        && !active.is_complete()
        && let Some((index, pattern, _)) = active.pending_step()
    {
        let reason = if timed_out {
            format!(
                "the case timed out after {} s before this step finished",
                spec.timeout.as_secs()
            )
        } else {
            "the process exited before this step finished".to_string()
        };
        step_failure = Some(StepFailure {
            index,
            pattern: pattern.to_string(),
            reason,
            lines: tail(&lines, 20),
        });
    }

    Ok(RunOutput {
        lines,
        status,
        timed_out,
        step_failure,
    })
}

/// Split a raw serial chunk into complete lines and feed the dialogue.
fn consume(
    chunk: &[u8],
    raw: &mut Vec<u8>,
    lines: &mut Vec<String>,
    dialogue: &mut Option<&mut Dialogue>,
) {
    for &byte in chunk {
        if byte == b'\n' {
            let line = clean_bytes(raw);
            raw.clear();
            if let Some(active) = dialogue.as_deref_mut() {
                active.line(&line, Instant::now());
            }
            lines.push(line);
        } else {
            raw.push(byte);
        }
    }
    if let Some(active) = dialogue.as_deref_mut() {
        let partial = clean_bytes(raw);
        active.partial(&partial, Instant::now());
    }
}

fn write_stdin(stdin: &mut Option<ChildStdin>, payload: &[u8]) {
    if let Some(pipe) = stdin.as_mut() {
        // A guest that exited already closes the pipe; the exit classification reports that.
        let _ = pipe.write_all(payload);
        let _ = pipe.flush();
    }
}

fn tail(lines: &[String], count: usize) -> Vec<String> {
    let start = lines.len().saturating_sub(count);
    lines[start..].to_vec()
}

fn clean_bytes(bytes: &[u8]) -> String {
    let mut line = String::from_utf8_lossy(bytes).into_owned();
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
    use crate::case::Case;
    use std::time::Instant;

    fn spec(script: &str, timeout: Duration) -> RunSpec {
        RunSpec {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            timeout,
        }
    }

    fn dialogue(step_tables: &str, timeout: u64) -> Dialogue {
        let text = format!(
            "name = \"d\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = {timeout}\n\
             expect_exit = \"success\"\n{step_tables}"
        );
        let case = Case::parse(&text).expect("case");
        Dialogue::from_case(&case).expect("dialogue")
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
        assert!(output.step_failure.is_none());
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

    #[test]
    fn dialogue_writes_to_the_guest_stdin() {
        let mut dialogue = dialogue(
            "[[step]]\nwait = \"^ready$\"\nsend = \"hello\\n\"\ndelay_ms = 0\n",
            5,
        );
        let output = run_dialogue(
            &spec(
                "printf 'ready\\n'; read line; printf 'got: %s\\n' \"$line\"; exit 33",
                Duration::from_secs(5),
            ),
            &mut dialogue,
        )
        .unwrap();
        assert!(output.step_failure.is_none(), "{:?}", output.step_failure);
        assert_eq!(output.status, Some(33));
        assert!(
            output.lines.contains(&"got: hello".to_string()),
            "{:?}",
            output.lines
        );
        assert!(dialogue.is_complete());
    }

    #[test]
    fn dialogue_matches_a_partial_prompt() {
        let mut dialogue = dialogue(
            "[[step]]\nwait = \"^raven# $\"\nsend = \"uname\\n\"\ndelay_ms = 0\n",
            5,
        );
        let output = run_dialogue(
            &spec(
                "printf 'raven# '; read line; printf 'got: %s\\n' \"$line\"; exit 33",
                Duration::from_secs(5),
            ),
            &mut dialogue,
        )
        .unwrap();
        assert!(output.step_failure.is_none(), "{:?}", output.step_failure);
        assert!(
            output.lines.iter().any(|line| line.contains("got: uname")),
            "{:?}",
            output.lines
        );
    }

    #[test]
    fn a_step_timeout_reports_the_index_pattern_and_lines() {
        let mut dialogue = dialogue("[[step]]\nwait = \"^never$\"\ntimeout_secs = 1\n", 10);
        let started = Instant::now();
        let script =
            "i=0; while [ $i -lt 30 ]; do printf 'line %s\\n' $i; i=$((i+1)); done; sleep 5";
        let output = run_dialogue(&spec(script, Duration::from_secs(10)), &mut dialogue).unwrap();
        let failure = output.step_failure.expect("step failure");
        assert_eq!(failure.index, 0);
        assert_eq!(failure.pattern, "^never$");
        assert_eq!(failure.lines.len(), 20);
        assert!(failure.lines[19].contains("line 29"), "{:?}", failure.lines);
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn a_process_exit_before_a_step_is_a_failure() {
        let mut dialogue = dialogue("[[step]]\nwait = \"^missing$\"\n", 5);
        let output = run_dialogue(
            &spec("printf 'other\\n'; exit 33", Duration::from_secs(5)),
            &mut dialogue,
        )
        .unwrap();
        let failure = output.step_failure.expect("step failure");
        assert_eq!(failure.index, 0);
        assert_eq!(failure.pattern, "^missing$");
        assert!(failure.reason.contains("exited"), "{}", failure.reason);
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
