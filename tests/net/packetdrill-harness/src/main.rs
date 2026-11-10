// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLI for the packetdrill-compatible harness.
//!
//! The parent process runs every script in a child process so the per-script timeout can be a
//! real hard kill. The child (`--run-one`) parses one script, drives `raven-netcore` in-process
//! over a `PairDevice`, and prints one JSON result object.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use raven_conform::result::{Results, Status, TestResult};
use raven_netcore_packetdrill_harness::{Runner, parse};
use serde::{Deserialize, Serialize};

/// Default per-script timeout, as required by the ticket.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ScriptResult {
    script: String,
    result: String,
    duration_ms: u64,
    detail: String,
}

impl ScriptResult {
    fn passed(&self) -> bool {
        self.result == "pass"
    }

    fn acceptable(&self) -> bool {
        self.result == "pass" || self.result == "known-fail"
    }
}

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("harness: {error:#}");
        std::process::exit(2);
    }
}

fn run_cli() -> Result<()> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if let Some(index) = arguments
        .iter()
        .position(|argument| argument == "--run-one")
    {
        let path = arguments
            .get(index + 1)
            .context("--run-one needs a script path")?;
        let timeout = timeout_argument(&arguments)?.unwrap_or(DEFAULT_TIMEOUT);
        let result = run_one(Path::new(path), timeout);
        println!("{}", serde_json::to_string(&result)?);
        std::io::stdout().flush().ok();
        std::process::exit(i32::from(!result.passed()));
    }

    let root = repository_root();
    let scripts = argument_value(&arguments, "--scripts")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("tests/net/scripts"));
    let results = argument_value(&arguments, "--results")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("tests/net/results"));
    let known = argument_value(&arguments, "--known-failures")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("tests/net/known-failures.txt"));
    let timeout = timeout_argument(&arguments)?.unwrap_or(DEFAULT_TIMEOUT);

    let known_failures = load_known_failures(&known)?;
    let mut script_paths = collect_scripts(&scripts)?;
    if script_paths.is_empty() {
        bail!("no .pkt scripts found in {}", scripts.display());
    }
    script_paths.sort();

    fs::create_dir_all(&results).with_context(|| format!("cannot create {}", results.display()))?;
    clear_old_results(&results)?;

    let mut outcomes = Vec::new();
    for path in &script_paths {
        let outcome = run_in_child(path, timeout, &known_failures)?;
        let json_path = results.join(format!(
            "{}.json",
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("script")
        ));
        fs::write(&json_path, serde_json::to_string_pretty(&outcome)?)
            .with_context(|| format!("cannot write {}", json_path.display()))?;
        outcomes.push(outcome);
    }

    write_conform_results(&results.join("conform.json"), &outcomes)?;
    write_summary(&results.join("summary.md"), &outcomes)?;

    let mut failures = 0usize;
    for outcome in &outcomes {
        println!(
            "{:<9} {:>6} ms  {}",
            outcome.result, outcome.duration_ms, outcome.script
        );
        if !outcome.acceptable() {
            failures += 1;
            eprintln!("{}: {}", outcome.script, outcome.detail);
        }
    }
    if failures > 0 {
        eprintln!("harness: {failures} of {} script(s) failed", outcomes.len());
        std::process::exit(1);
    }
    Ok(())
}

fn run_one(path: &Path, timeout: Duration) -> ScriptResult {
    let started = Instant::now();
    let script = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("script.pkt")
        .to_string();
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            return ScriptResult {
                script,
                result: "fail".to_string(),
                duration_ms: elapsed_ms(started),
                detail: format!("cannot read {}: {error}", path.display()),
            };
        }
    };
    match parse(&script, &text) {
        Ok(parsed) => match Runner::new(parsed).run(timeout) {
            Ok(()) => ScriptResult {
                script,
                result: "pass".to_string(),
                duration_ms: elapsed_ms(started),
                detail: "ok".to_string(),
            },
            Err(error) => ScriptResult {
                script,
                result: "fail".to_string(),
                duration_ms: elapsed_ms(started),
                detail: format!("{error:#}"),
            },
        },
        Err(error) => ScriptResult {
            script,
            result: "fail".to_string(),
            duration_ms: elapsed_ms(started),
            detail: format!("parse: {error:#}"),
        },
    }
}

fn run_in_child(
    path: &Path,
    timeout: Duration,
    known_failures: &BTreeMap<String, String>,
) -> Result<ScriptResult> {
    let executable = env::current_exe().context("cannot locate the harness executable")?;
    let budget = timeout.saturating_add(Duration::from_secs(1));
    let mut child = Command::new(executable)
        .arg("--run-one")
        .arg(path)
        .arg("--timeout-ms")
        .arg(timeout.as_millis().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot spawn the script child")?;

    let stdout = child.stdout.take().context("missing child stdout")?;
    let stderr = child.stderr.take().context("missing child stderr")?;
    let (sender, receiver) = mpsc::channel();
    let stdout_reader = thread::spawn(move || {
        let mut text = String::new();
        let mut reader = stdout;
        let result = reader.read_to_string(&mut text).map(|_| text);
        let _ = sender.send(result);
    });
    let stderr_reader = thread::spawn(move || {
        let mut text = String::new();
        let mut reader = stderr;
        let _ = reader.read_to_string(&mut text);
        text
    });

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().context("cannot poll the script child")? {
            break Some(status);
        }
        if started.elapsed() >= budget {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(2));
    };

    let stdout_text = receiver
        .recv_timeout(Duration::from_secs(2))
        .ok()
        .and_then(std::result::Result::ok)
        .unwrap_or_default();
    let _ = stdout_reader.join();
    let stderr_text = stderr_reader.join().unwrap_or_default();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("script.pkt")
        .to_string();

    let Some(status) = status else {
        return Ok(ScriptResult {
            script: name,
            result: "fail".to_string(),
            duration_ms: elapsed_ms(started),
            detail: format!("hard timeout after {} ms", budget.as_millis()),
        });
    };

    let mut outcome = match serde_json::from_str::<ScriptResult>(stdout_text.trim()) {
        Ok(outcome) => outcome,
        Err(error) => ScriptResult {
            script: name,
            result: "fail".to_string(),
            duration_ms: elapsed_ms(started),
            detail: format!(
                "child produced no result ({error}); exit {status}; stderr: {}",
                stderr_text.trim()
            ),
        },
    };

    if let Some(reason) = known_failures.get(&outcome.script) {
        if outcome.passed() {
            outcome.result = "fail".to_string();
            outcome.detail = format!(
                "stale known-fail entry: the script passed; remove it from known-failures.txt ({reason})"
            );
        } else {
            outcome.result = "known-fail".to_string();
            outcome.detail = format!("{}; known-fail: {reason}", outcome.detail);
        }
    }
    Ok(outcome)
}

fn timeout_argument(arguments: &[String]) -> Result<Option<Duration>> {
    let Some(value) = argument_value(arguments, "--timeout-ms") else {
        return Ok(None);
    };
    let millis: u64 = value.parse().context("--timeout-ms must be an integer")?;
    if millis == 0 {
        bail!("--timeout-ms must be greater than zero");
    }
    Ok(Some(Duration::from_millis(millis)))
}

fn argument_value(arguments: &[String], name: &str) -> Option<String> {
    arguments
        .iter()
        .position(|argument| argument == name)
        .and_then(|index| arguments.get(index + 1))
        .cloned()
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn collect_scripts(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut scripts = Vec::new();
    for entry in
        fs::read_dir(directory).with_context(|| format!("cannot read {}", directory.display()))?
    {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "pkt") {
            scripts.push(path);
        }
    }
    Ok(scripts)
}

fn clear_old_results(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let is_result = path
            .extension()
            .is_some_and(|extension| extension == "json" || extension == "md");
        if path.is_file() && is_result {
            fs::remove_file(&path)
                .with_context(|| format!("cannot remove stale {}", path.display()))?;
        }
    }
    Ok(())
}

fn load_known_failures(path: &Path) -> Result<BTreeMap<String, String>> {
    let mut known = BTreeMap::new();
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(known);
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, reason) = line
            .split_once(char::is_whitespace)
            .with_context(|| format!("{}: known failure needs a reason", path.display()))?;
        known.insert(name.trim().to_string(), reason.trim().to_string());
    }
    Ok(known)
}

fn write_conform_results(path: &Path, outcomes: &[ScriptResult]) -> Result<()> {
    let mut results = Results {
        tool: "packetdrill-harness".to_string(),
        tool_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        target: Some("raven-netcore".to_string()),
        unparsed: 0,
        results: outcomes
            .iter()
            .map(|outcome| TestResult {
                suite: "packetdrill".to_string(),
                name: outcome.script.clone(),
                status: match outcome.result.as_str() {
                    "pass" => Status::Pass,
                    "known-fail" => Status::Skip,
                    "broken" => Status::Broken,
                    _ => Status::Fail,
                },
                duration_ms: Some(outcome.duration_ms),
                message: Some(outcome.detail.clone()),
            })
            .collect(),
    };
    results.normalize();
    fs::write(path, serde_json::to_string_pretty(&results)?)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn write_summary(path: &Path, outcomes: &[ScriptResult]) -> Result<()> {
    let passed = outcomes.iter().filter(|outcome| outcome.passed()).count();
    let known = outcomes
        .iter()
        .filter(|outcome| outcome.result == "known-fail")
        .count();
    let failed = outcomes.len() - passed - known;
    let mut text = String::new();
    text.push_str("# packetdrill harness summary\n\n");
    text.push_str(
        "Topology: SUT `10.0.0.1:8080` (`02:00:00:00:00:01`), peer `10.0.0.2:12345` \
         (`02:00:00:00:00:02`), peer ISN 1000, in-memory `PairDevice`.\n\n",
    );
    text.push_str("| Script | Result | Duration (ms) | Notes |\n");
    text.push_str("|---|---|---|---|\n");
    for outcome in outcomes {
        let notes = outcome.detail.replace('|', "\\|").replace('\n', " ");
        text.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            outcome.script, outcome.result, outcome.duration_ms, notes
        ));
    }
    text.push_str(&format!(
        "\nTotals: {} script(s), {} pass, {} known-fail, {} fail.\n",
        outcomes.len(),
        passed,
        known,
        failed
    ));
    fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}
