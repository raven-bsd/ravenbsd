// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process execution with serial capture, scripted dialogue, ANSI stripping and a hard timeout.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::{Value, json};

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

/// A `send` payload beginning with `@qmp:<socket>:<key,...>` is a QMP key chord, not serial text.
///
/// The case supplies the QMP socket through `qemu_extra`:
/// `-qmp unix:build/qtest/<case>.qmp,server=on,wait=off`. This keeps QEMU's serial stdio
/// untouched and gives the harness a side channel for `send-key`.
/// Extract the QMP socket path from the QEMU argument vector.
fn qmp_path(args: &[String]) -> Option<PathBuf> {
    for pair in args.windows(2) {
        if pair[0] != "-qmp" {
            continue;
        }
        let path = pair[1].strip_prefix("unix:")?;
        let path = path.split(',').next()?;
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn qmp_action(payload: &[u8]) -> Option<(PathBuf, Vec<String>)> {
    let text = core::str::from_utf8(payload).ok()?;
    let rest = text.strip_prefix("@qmp:")?;
    let (path, keys) = rest.split_once(':')?;
    if path.is_empty() || keys.is_empty() {
        return None;
    }
    let keys: Vec<String> = keys
        .split(',')
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .collect();
    if keys.is_empty() {
        return None;
    }
    Some((PathBuf::from(path), keys))
}

/// A small QMP client for the `send-key` action.
struct QmpClient {
    stream: std::os::unix::net::UnixStream,
    reader: BufReader<std::os::unix::net::UnixStream>,
}

impl QmpClient {
    fn connect(path: &PathBuf) -> Result<Self> {
        let mut last = None;
        for _ in 0..300 {
            match std::os::unix::net::UnixStream::connect(path) {
                Ok(stream) => {
                    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                    let mut client = Self {
                        reader: BufReader::new(stream.try_clone()?),
                        stream,
                    };
                    client.handshake()?;
                    return Ok(client);
                }
                Err(error) => {
                    last = Some(error);
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
        Err(anyhow::anyhow!(
            "QMP socket did not appear: {}",
            last.map(|error| error.to_string())
                .unwrap_or_else(|| "unknown error".to_string())
        ))
    }

    fn handshake(&mut self) -> Result<()> {
        let mut greeting = String::new();
        self.reader.read_line(&mut greeting)?;
        if greeting.trim().is_empty() {
            bail!("QMP greeting is empty");
        }
        self.command(json!({"execute": "qmp_capabilities"}))
    }

    fn send_key(&mut self, keys: &[String]) -> Result<()> {
        let keys: Vec<Value> = keys
            .iter()
            .map(|key| json!({"type": "qcode", "data": key}))
            .collect();
        self.command(json!({
            "execute": "send-key",
            "arguments": {"keys": keys, "hold-time": 500}
        }))
    }

    fn command(&mut self, command: Value) -> Result<()> {
        let mut line = serde_json::to_vec(&command)?;
        line.push(b'\n');
        self.stream.write_all(&line)?;
        self.stream.flush()?;
        loop {
            let mut response = String::new();
            let read = self.reader.read_line(&mut response)?;
            if read == 0 {
                bail!("QMP connection closed");
            }
            let response: Value = serde_json::from_str(response.trim())?;
            if let Some(error) = response.get("error") {
                bail!("QMP error: {error}");
            }
            if response.get("return").is_some() {
                return Ok(());
            }
        }
    }
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
    let mut qmp = match qmp_path(&spec.args) {
        Some(path) => Some(
            QmpClient::connect(&path)
                .with_context(|| format!("connecting to QMP socket {}", path.display()))?,
        ),
        None => None,
    };
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
            let pending = active
                .pending_step()
                .map(|(index, pattern, _)| (index, pattern.to_string()));
            if let Some(payload) = active.poll_send(now) {
                let mut action_error = None;
                if let Some((path, keys)) = qmp_action(&payload) {
                    if qmp.is_none() {
                        match QmpClient::connect(&path) {
                            Ok(client) => qmp = Some(client),
                            Err(error) => action_error = Some(format!("QMP connect: {error:#}")),
                        }
                    }
                    if action_error.is_none()
                        && let Some(client) = qmp.as_mut()
                        && let Err(error) = client.send_key(&keys)
                    {
                        action_error = Some(format!("QMP send-key: {error:#}"));
                    }
                } else {
                    write_stdin(&mut stdin, &payload);
                }
                if let Some(reason) = action_error {
                    let (index, pattern) = pending.unwrap_or_else(|| (0, "qmp".to_string()));
                    step_failure = Some(StepFailure {
                        index,
                        pattern,
                        reason,
                        lines: tail(&lines, 20),
                    });
                    kill_tree(&mut child);
                    break;
                }
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
#[path = "runner_ut.rs"]
mod tests;
