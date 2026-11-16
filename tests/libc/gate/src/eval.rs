// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The gate evaluator: run every row, write evidence, aggregate statuses.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::man;
use crate::model::{CheckResult, Status, aggregate};
use crate::rows::{CheckSpec, Rows};
use crate::soak;
use anyhow::{Context, Result, bail};

/// Evaluator options.
#[derive(Clone, Debug)]
pub struct Options {
    /// Repository root.
    pub repo: PathBuf,
    /// Evidence directory.
    pub out: PathBuf,
    /// Total timeout per command.
    pub timeout: Duration,
    /// Optional id filter (exact or prefix).
    pub only: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            repo: PathBuf::from("."),
            out: PathBuf::from("build/libc-gate"),
            timeout: Duration::from_secs(3600),
            only: None,
        }
    }
}

/// Captured process result.
#[derive(Clone, Debug, Default)]
pub struct Process {
    /// Exit code; `None` for a signal or timeout.
    pub exit: Option<i32>,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// True when the timeout killed the process.
    pub timed_out: bool,
}

/// Run one argument vector in `repo`.
///
/// # Errors
///
/// Returns an error when the process cannot be spawned or its pipes cannot be read.
pub fn run_process(repo: &Path, argv: &[String], timeout: Duration) -> Result<Process> {
    if argv.is_empty() {
        bail!("empty argv");
    }
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(repo)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("cannot spawn {}", argv[0]))?;
    let stdout = child.stdout.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    });
    let stderr = child.stderr.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    Ok(Process {
        exit: status.and_then(|status| status.code()),
        stdout,
        stderr,
        timed_out,
    })
}

fn ensure_out(out: &Path) -> Result<()> {
    fs::create_dir_all(out).with_context(|| format!("cannot create {}", out.display()))
}

fn evidence(
    out: &Path,
    id: &str,
    argv: Option<&[String]>,
    process: Option<&Process>,
    status: Status,
    reason: &str,
) -> Result<String> {
    ensure_out(out)?;
    let path = out.join(format!("{id}.txt"));
    let mut text = String::new();
    match argv {
        Some(argv) => text.push_str(&format!("$ {}\n", argv.join(" "))),
        None => text.push_str("$ (not run)\n"),
    }
    match process {
        Some(process) => {
            text.push_str(&format!(
                "exit: {}\n",
                process
                    .exit
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "signal".to_string())
            ));
            text.push_str("--- stdout ---\n");
            text.push_str(&process.stdout);
            if !process.stdout.ends_with('\n') {
                text.push('\n');
            }
            text.push_str("--- stderr ---\n");
            text.push_str(&process.stderr);
            if !process.stderr.ends_with('\n') {
                text.push('\n');
            }
        }
        None => text.push_str("exit: n/a\n"),
    }
    text.push_str(&format!("status: {} ({reason})\n", status));
    fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path.to_string_lossy().to_string())
}

#[allow(clippy::too_many_arguments)]
fn record(
    out: &Path,
    spec: &CheckSpec,
    argv: Option<&[String]>,
    process: Option<&Process>,
    status: Status,
    reason: String,
) -> Result<CheckResult> {
    let path = evidence(out, &spec.id, argv, process, status, &reason)?;
    let result = CheckResult {
        id: spec.id.clone(),
        row: spec.row,
        title: spec.title.clone(),
        status,
        exit: process.and_then(|process| process.exit),
        reason,
        evidence: path,
    };
    let json = serde_json::to_string_pretty(&result)?;
    fs::write(out.join(format!("{}.json", spec.id)), json)?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn record_child(
    out: &Path,
    parent: &CheckSpec,
    id: &str,
    title: &str,
    argv: Option<&[String]>,
    process: Option<&Process>,
    status: Status,
    reason: String,
) -> Result<CheckResult> {
    let path = evidence(out, id, argv, process, status, &reason)?;
    let result = CheckResult {
        id: id.to_string(),
        row: parent.row,
        title: title.to_string(),
        status,
        exit: process.and_then(|process| process.exit),
        reason,
        evidence: path,
    };
    fs::write(
        out.join(format!("{id}.json")),
        serde_json::to_string_pretty(&result)?,
    )?;
    Ok(result)
}

fn status_from_exit(spec: &CheckSpec, process: &Process) -> Status {
    let Some(exit) = process.exit else {
        return Status::Blocked;
    };
    if spec.blocked_on.exit.contains(&exit) {
        Status::Blocked
    } else if exit == spec.pass.exit {
        Status::Met
    } else {
        Status::NotImplemented
    }
}

fn reason_from_exit(spec: &CheckSpec, process: &Process) -> String {
    if process.timed_out {
        return "timed out".to_string();
    }
    match process.exit {
        Some(exit) if exit == spec.pass.exit => format!("exit {exit}"),
        Some(exit) if spec.blocked_on.exit.contains(&exit) => {
            format!("exit {exit}: prerequisite absent")
        }
        Some(exit) => format!("exit {exit}: expected {}", spec.pass.exit),
        None => "terminated by signal".to_string(),
    }
}

/// Find the ticket index in the repository or the near ancestry.
fn ticket_index(repo: &Path) -> Option<PathBuf> {
    let mut directory = Some(repo);
    while let Some(current) = directory {
        for candidate in [
            current.join("tickets/INDEX.md"),
            current.join("ravenbsd-prjmgmt/tickets/INDEX.md"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        directory = current.parent();
    }
    None
}

/// Read a ticket state from `tickets/INDEX.md`.
#[must_use]
pub fn ticket_state(repo: &Path, id: &str) -> Option<String> {
    let path = ticket_index(repo)?;
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.first() == Some(&id) {
            return cells.last().map(|state| (*state).to_string());
        }
    }
    None
}

fn blocked_reason(repo: &Path, spec: &CheckSpec) -> Option<String> {
    if let Some(ticket) = &spec.blocked_on.ticket {
        match ticket_state(repo, ticket) {
            Some(state) if state == "done" => {}
            Some(state) => return Some(format!("{ticket} is {state}")),
            None => return Some(format!("{ticket} not done (INDEX unavailable)")),
        }
    }
    for file in &spec.blocked_on.missing_files {
        if !repo.join(file).exists() {
            return Some(format!("missing {file}"));
        }
    }
    None
}

fn run_blocked(spec: &CheckSpec, out: &Path, reason: String) -> Result<Vec<CheckResult>> {
    let reason = match &spec.note {
        Some(note) => format!("{reason}; {note}"),
        None => reason,
    };
    Ok(vec![record(
        out,
        spec,
        None,
        None,
        Status::Blocked,
        reason,
    )?])
}

fn read_file(repo: &Path, spec: &CheckSpec) -> Result<String> {
    let file = spec
        .file
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("check {} has no file", spec.id))?;
    fs::read_to_string(repo.join(file)).with_context(|| format!("cannot read {file}"))
}

fn expected_fuzz_targets(repo: &Path) -> Vec<String> {
    let directory = repo.join("tests/libc/fuzz/fuzz_targets");
    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("rs")
            && let Some(stem) = path.file_stem().and_then(|value| value.to_str())
        {
            targets.push(stem.to_string());
        }
    }
    targets.sort();
    targets
}

fn soak_check(repo: &Path, spec: &CheckSpec) -> Result<(Status, String)> {
    let text = read_file(repo, spec)?;
    let rows = soak::parse(&text);
    let known = fs::read_to_string(repo.join("tests/libc/fuzz/known-gaps.md")).unwrap_or_default();
    let deferred = soak::deferred_targets(&known);
    let mut expected = expected_fuzz_targets(repo);
    if expected.is_empty() {
        expected = rows.iter().map(|row| row.target.clone()).collect();
    }
    let min = spec.min_seconds.unwrap_or(3600);
    let report = soak::evaluate(&rows, &expected, &deferred, min);
    Ok((report.status, report.reason))
}

fn qtest_status(repo: &Path, case: &str) -> Result<(Status, String)> {
    let path = repo.join(format!("build/qtest/{case}.json"));
    if !path.is_file() {
        return Ok((Status::Blocked, format!("missing {}", path.display())));
    }
    let text = fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    match value.get("result").and_then(|value| value.as_str()) {
        Some("pass") => Ok((Status::Met, "passed".to_string())),
        Some("skipped") => Ok((Status::Blocked, "qtest skipped".to_string())),
        Some("fail") => Ok((Status::NotImplemented, "qtest failed".to_string())),
        Some(other) => Ok((Status::NotImplemented, format!("qtest result {other}"))),
        None => Ok((Status::Blocked, "qtest JSON has no result".to_string())),
    }
}

fn hardened_malloc(repo: &Path, timeout: Duration) -> Result<(Status, String)> {
    let manifest = repo.join("libc/ravenlibc/crates/raven-libc/Cargo.toml");
    let text = fs::read_to_string(&manifest)
        .with_context(|| format!("cannot read {}", manifest.display()))?;
    let value: toml::Value = toml::from_str(&text)?;
    let features = value
        .get("features")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| anyhow::anyhow!("raven-libc Cargo.toml has no features"))?;
    let has = |name: &str, needle: &str| {
        features
            .get(name)
            .and_then(toml::Value::as_array)
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(needle)))
    };
    if !has("default", "base") {
        return Ok((
            Status::NotImplemented,
            "default does not contain base".to_string(),
        ));
    }
    if !has("base", "hardened-malloc") {
        return Ok((
            Status::NotImplemented,
            "base does not contain hardened-malloc".to_string(),
        ));
    }
    let map = repo.join("build/x86_64/libc.a.map.json");
    if !map.is_file() {
        return Ok((Status::Blocked, "libc.a.map.json is missing".to_string()));
    }
    let map_text = fs::read_to_string(&map)?;
    let hardened = map_text.contains("heap_backend")
        || map_text.contains("hardened")
        || map_text.contains("malloc4heap");
    if !hardened {
        return Ok((
            Status::NotImplemented,
            "libc.a map has no hardened backend member".to_string(),
        ));
    }
    let so_path = repo.join("build/x86_64/libc.so.1");
    if !so_path.is_file() {
        return Ok((Status::Blocked, "libc.so.1 is missing".to_string()));
    }
    let nm = run_process(
        repo,
        &[
            "nm".to_string(),
            "-D".to_string(),
            "--defined-only".to_string(),
            "build/x86_64/libc.so.1".to_string(),
        ],
        timeout,
    )?;
    if nm.exit != Some(0) {
        return Ok((Status::Blocked, "nm could not read libc.so.1".to_string()));
    }
    if nm.stdout.contains("tiny_malloc") || nm.stdout.contains("tiny-malloc") {
        return Ok((
            Status::NotImplemented,
            "tiny allocator appears in libc.so.1".to_string(),
        ));
    }
    Ok((
        Status::Met,
        "hardened backend selected; tiny not in libc.so.1".to_string(),
    ))
}

fn entropy_weak(repo: &Path) -> (Status, String) {
    let root = repo.join("libc/ravenlibc/crates/raven-libc-rand");
    let mut open = false;
    if root.is_dir() {
        for entry in fs::read_dir(&root).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path)
                && text.contains("ENTROPY_WEAK_BUILD")
                && text.contains("weak-entropy-interim")
            {
                open = true;
            }
        }
    }
    let log = fs::read_to_string(repo.join("build/qtest/libc-hello.log")).unwrap_or_default();
    let state = if open || log.contains("entropy weak=weak") {
        "OPEN"
    } else if log.contains("entropy weak=") {
        "CLEAR"
    } else {
        "UNVERIFIED"
    };
    (
        Status::Info,
        format!("ENTROPY_WEAK {state}; PG-52 release blocker, not a libc-gate blocker"),
    )
}

fn reader(repo: &Path, spec: &CheckSpec, timeout: Duration) -> Result<(Status, String)> {
    match spec.read.as_deref() {
        Some("soak") => soak_check(repo, spec),
        Some("hardened-malloc") => hardened_malloc(repo, timeout),
        Some("entropy-weak") => Ok(entropy_weak(repo)),
        Some("qtest") => {
            let mut statuses = Vec::new();
            let mut reasons = Vec::new();
            for case in &spec.cases {
                let (status, reason) = qtest_status(repo, case)?;
                statuses.push(status);
                reasons.push(format!("{case}: {reason}"));
            }
            Ok((aggregate(statuses), reasons.join("; ")))
        }
        Some("man") => {
            let report = man::check_default(repo)?;
            Ok(if report.is_ok() {
                (
                    Status::Met,
                    format!(
                        "{} candidates covered by {} pages",
                        report.candidates.len(),
                        report.pages
                    ),
                )
            } else {
                (Status::NotImplemented, report.errors.join("; "))
            })
        }
        Some("files") => {
            let file = spec.file.as_deref().unwrap_or("");
            let exists = repo.join(file).is_file();
            Ok(if exists {
                (Status::Met, format!("{file} exists"))
            } else {
                (Status::Blocked, format!("missing {file}"))
            })
        }
        Some("mandoc") => {
            if spec.cases.is_empty() {
                return Ok((Status::Blocked, "no mandoc page list".to_string()));
            }
            let mut argv = vec![
                "mandoc".to_string(),
                "-Tlint".to_string(),
                "-Wwarning".to_string(),
            ];
            argv.extend(spec.cases.iter().cloned());
            let process = run_process(repo, &argv, timeout)?;
            Ok((
                status_from_exit(spec, &process),
                reason_from_exit(spec, &process),
            ))
        }
        Some(other) => Ok((Status::Blocked, format!("unknown reader {other}"))),
        None => Ok((Status::Blocked, "no reader".to_string())),
    }
}

fn run_argv(repo: &Path, spec: &CheckSpec, timeout: Duration) -> Result<Process> {
    run_process(repo, &spec.argv, timeout)
}

fn substitute(argv: &[String], needle: &str, value: &str) -> Vec<String> {
    argv.iter().map(|arg| arg.replace(needle, value)).collect()
}

fn eval_oracle(
    repo: &Path,
    spec: &CheckSpec,
    out: &Path,
    timeout: Duration,
) -> Result<Vec<CheckResult>> {
    let list = run_process(repo, &spec.argv, timeout)?;
    if spec.blocked_on.exit.contains(&list.exit.unwrap_or(-1)) || list.exit != Some(spec.pass.exit)
    {
        return Ok(vec![record(
            out,
            spec,
            Some(&spec.argv),
            Some(&list),
            Status::Blocked,
            "oracle list command did not succeed".to_string(),
        )?]);
    }
    let families: Vec<String> = list
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with("Finished") && !line.starts_with("Running")
        })
        .map(str::to_string)
        .collect();
    if families.is_empty() {
        return Ok(vec![record(
            out,
            spec,
            Some(&spec.argv),
            Some(&list),
            Status::Blocked,
            "oracle list produced no families".to_string(),
        )?]);
    }
    let mut results = Vec::new();
    for family in families {
        let id = format!("{}.{}", spec.id, family);
        let argv = substitute(&spec.child_argv, "{family}", &family);
        let process = run_process(repo, &argv, timeout)?;
        let status = status_from_exit(spec, &process);
        results.push(record_child(
            out,
            spec,
            &id,
            &format!("{} {family}", spec.title),
            Some(&argv),
            Some(&process),
            status,
            reason_from_exit(spec, &process),
        )?);
    }
    let status = aggregate(results.iter().map(|result| result.status));
    let parent = record(
        out,
        spec,
        Some(&spec.argv),
        Some(&list),
        status,
        format!("{} oracle families", results.len()),
    )?;
    results.push(parent);
    Ok(results)
}

fn eval_crates(
    repo: &Path,
    spec: &CheckSpec,
    out: &Path,
    timeout: Duration,
) -> Result<Vec<CheckResult>> {
    let manifest = repo.join("libc/ravenlibc/xtask/crates.toml");
    let text = fs::read_to_string(&manifest)
        .with_context(|| format!("cannot read {}", manifest.display()))?;
    let value: toml::Value = toml::from_str(&text)?;
    let crates = value
        .get("crate")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("crates.toml has no crate array"))?;
    let names: Vec<String> = crates
        .iter()
        .filter(|entry| entry.get("host").and_then(toml::Value::as_bool) == Some(true))
        .filter_map(|entry| entry.get("name").and_then(toml::Value::as_str))
        .map(str::to_string)
        .collect();
    if names.is_empty() {
        return Ok(vec![record(
            out,
            spec,
            Some(&spec.argv),
            None,
            Status::Blocked,
            "crates.toml has no host crates".to_string(),
        )?]);
    }
    let mut results = Vec::new();
    for name in names {
        let id = format!("{}.{}", spec.id, name);
        let argv = substitute(&spec.child_argv, "{crate}", &name);
        let process = run_process(repo, &argv, timeout)?;
        let status = status_from_exit(spec, &process);
        results.push(record_child(
            out,
            spec,
            &id,
            &format!("{} {name}", spec.title),
            Some(&argv),
            Some(&process),
            status,
            reason_from_exit(spec, &process),
        )?);
    }
    let status = aggregate(results.iter().map(|result| result.status));
    let parent = record(
        out,
        spec,
        Some(&spec.argv),
        None,
        status,
        format!("{} host crates", results.len()),
    )?;
    results.push(parent);
    Ok(results)
}

fn eval_qtest_expand(
    repo: &Path,
    spec: &CheckSpec,
    out: &Path,
    timeout: Duration,
) -> Result<Vec<CheckResult>> {
    let mut results = Vec::new();
    for case in &spec.cases {
        let id = format!("{}.{}", spec.id, case);
        let argv = substitute(&spec.child_argv, "{case}", case);
        let process = run_process(repo, &argv, timeout)?;
        let (status, reason) = if process.exit == Some(spec.pass.exit) {
            qtest_status(repo, case)?
        } else if spec.blocked_on.exit.contains(&process.exit.unwrap_or(-1)) {
            (Status::Blocked, "qtest exited 77".to_string())
        } else {
            (Status::NotImplemented, reason_from_exit(spec, &process))
        };
        results.push(record_child(
            out,
            spec,
            &id,
            &format!("{} {case}", spec.title),
            Some(&argv),
            Some(&process),
            status,
            reason,
        )?);
    }
    let status = aggregate(results.iter().map(|result| result.status));
    let parent = record(
        out,
        spec,
        Some(&spec.argv),
        None,
        status,
        format!("{} qtest cases", results.len()),
    )?;
    results.push(parent);
    Ok(results)
}

fn eval_one(
    repo: &Path,
    spec: &CheckSpec,
    out: &Path,
    timeout: Duration,
) -> Result<Vec<CheckResult>> {
    if let Some(reason) = blocked_reason(repo, spec) {
        return run_blocked(spec, out, reason);
    }
    if !spec.preflight.is_empty() {
        let process = run_process(repo, &spec.preflight, timeout)?;
        if process.exit != Some(0) {
            return Ok(vec![record(
                out,
                spec,
                Some(&spec.preflight),
                Some(&process),
                Status::Blocked,
                "preflight command failed".to_string(),
            )?]);
        }
    }
    if spec.kind == "info" {
        let (status, reason) = if let Some(kind) = spec.read.as_deref() {
            let _ = kind;
            reader(repo, spec, timeout)?
        } else {
            (
                Status::Info,
                spec.note.clone().unwrap_or_else(|| "recorded".to_string()),
            )
        };
        return Ok(vec![record(
            out,
            spec,
            Some(&spec.argv),
            None,
            status,
            reason,
        )?]);
    }
    if spec.kind == "read" {
        let (status, reason) = reader(repo, spec, timeout)?;
        // An INFO reader must stay INFO even when the underlying observation is CLEAR/OPEN.
        let status = if spec.required { status } else { Status::Info };
        return Ok(vec![record(out, spec, None, None, status, reason)?]);
    }
    if let Some(expand) = &spec.expand {
        return match expand.as_str() {
            "oracle" => eval_oracle(repo, spec, out, timeout),
            "crates" => eval_crates(repo, spec, out, timeout),
            "qtest" => eval_qtest_expand(repo, spec, out, timeout),
            other => Ok(vec![record(
                out,
                spec,
                Some(&spec.argv),
                None,
                Status::Blocked,
                format!("unknown expansion {other}"),
            )?]),
        };
    }
    let process = run_argv(repo, spec, timeout)?;
    let status = status_from_exit(spec, &process);
    Ok(vec![record(
        out,
        spec,
        Some(&spec.argv),
        Some(&process),
        status,
        reason_from_exit(spec, &process),
    )?])
}

/// Evaluate the selected checks and write their evidence.
///
/// # Errors
///
/// Returns an error for unreadable inputs or evidence that cannot be written.
pub fn eval(rows: &Rows, options: &Options) -> Result<Vec<CheckResult>> {
    ensure_out(&options.out)?;
    let mut results = Vec::new();
    for spec in &rows.check {
        if let Some(only) = &options.only {
            let selected = only.split(',').any(|filter| {
                let filter = filter.trim();
                spec.id == filter || spec.id.starts_with(&format!("{filter}."))
            });
            if !selected {
                continue;
            }
        }
        results.extend(eval_one(
            &options.repo,
            spec,
            &options.out,
            options.timeout,
        )?);
    }
    Ok(results)
}

#[cfg(test)]
#[path = "eval_ut.rs"]
mod tests;
