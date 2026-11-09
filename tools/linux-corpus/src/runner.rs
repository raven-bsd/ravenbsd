// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Corpus execution: `requires` checks, setup commands, `strace` and timeouts.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::corpus::{Corpus, Program};
use crate::strace::{SyscallStat, parse_trace};

/// Options for one `run` invocation.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Corpus file to load.
    pub corpus: PathBuf,
    /// Directory for traces, per-program JSON and scratch work dirs.
    pub out: PathBuf,
    /// Optional program ids; empty means all programs.
    pub only: Vec<String>,
}

/// Result of one corpus program.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramResult {
    /// Program id.
    pub id: String,
    /// Program category.
    pub category: String,
    /// Actual exit status, or `None` for skipped/setup-failed programs.
    pub exit: Option<i32>,
    /// Exit status the corpus expected.
    pub expected_exit: i32,
    /// `requires` was missing.
    pub skipped: bool,
    /// Human-readable skip/failure reason.
    pub reason: Option<String>,
    /// The timeout fired.
    pub timed_out: bool,
    /// A setup command failed.
    pub setup_failed: bool,
    /// The trace could not be read or was not valid UTF-8.
    #[serde(default)]
    pub capture_error: Option<String>,
    /// Wall-clock duration of setup plus the traced run.
    pub duration_ms: u64,
    /// Number of distinct syscalls parsed from the trace.
    pub syscalls_used: usize,
    /// Trace path (relative to the repository or absolute as given).
    pub strace: String,
    /// Parsed syscall aggregates.
    pub syscalls: BTreeMap<String, SyscallStat>,
    /// Unknown/odd trace lines.
    pub unparsed_lines: u64,
}

impl ProgramResult {
    /// Human status used by the summary and the dashboard.
    pub fn status(&self) -> &'static str {
        if self.skipped {
            "skipped"
        } else if self.setup_failed {
            "setup-failed"
        } else if self.timed_out {
            "timeout"
        } else if self.capture_error.is_some() {
            "capture-failed"
        } else if self.exit == Some(self.expected_exit) {
            "ok"
        } else {
            "failed"
        }
    }

    /// Whether this result represents a successful program execution.
    pub fn is_ok(&self) -> bool {
        !self.skipped
            && !self.setup_failed
            && !self.timed_out
            && self.capture_error.is_none()
            && self.exit == Some(self.expected_exit)
    }
}

/// Run the selected corpus programs.
pub fn run_corpus(options: &RunOptions) -> Result<Vec<ProgramResult>> {
    let corpus_path = fs::canonicalize(&options.corpus)
        .with_context(|| format!("resolving corpus {}", options.corpus.display()))?;
    let corpus = Corpus::load(&corpus_path)?;
    let corpus_dir = corpus_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let selected = select_programs(&corpus, &options.only)?;
    fs::create_dir_all(&options.out)
        .with_context(|| format!("creating output directory {}", options.out.display()))?;
    let out = fs::canonicalize(&options.out)
        .with_context(|| format!("resolving output directory {}", options.out.display()))?;

    let mut results = Vec::new();
    for program in selected {
        results.push(run_program(program, &corpus_dir, &out)?);
    }
    Ok(results)
}

fn select_programs<'a>(corpus: &'a Corpus, only: &[String]) -> Result<Vec<&'a Program>> {
    if only.is_empty() {
        return Ok(corpus.programs.iter().collect());
    }
    let mut selected = Vec::new();
    for id in only {
        let program = corpus
            .programs
            .iter()
            .find(|program| program.id == *id)
            .ok_or_else(|| anyhow::anyhow!("unknown program id `{id}`"))?;
        if !selected
            .iter()
            .any(|entry: &&Program| entry.id == program.id)
        {
            selected.push(program);
        }
    }
    Ok(selected)
}

fn run_program(program: &Program, corpus_dir: &Path, out: &Path) -> Result<ProgramResult> {
    let started = Instant::now();
    let work = out.join("work").join(&program.id);
    if work.exists() {
        fs::remove_dir_all(&work)
            .with_context(|| format!("removing stale work dir {}", work.display()))?;
    }
    fs::create_dir_all(&work).with_context(|| format!("creating work dir {}", work.display()))?;
    let trace_path = out.join(format!("{}.strace", program.id));

    if let Some(missing) = missing_requirement(&program.requires) {
        let result = ProgramResult {
            id: program.id.clone(),
            category: program.category.clone(),
            exit: None,
            expected_exit: program.expect_exit,
            skipped: true,
            reason: Some(format!("not installed: {missing}")),
            timed_out: false,
            setup_failed: false,
            capture_error: None,
            duration_ms: elapsed_ms(started),
            syscalls_used: 0,
            strace: trace_path.display().to_string(),
            syscalls: BTreeMap::new(),
            unparsed_lines: 0,
        };
        write_result(out, &result)?;
        let _ = fs::remove_dir_all(&work);
        return Ok(result);
    }

    let timeout = Duration::from_secs(program.timeout_secs);
    let env = program.expanded_env(&work, corpus_dir);
    for command in program.expanded_setup(&work, corpus_dir) {
        let outcome = execute(&command[0], &command[1..], &env, None, timeout, &work)
            .with_context(|| format!("running setup for `{}`", program.id))?;
        if outcome.timed_out || outcome.status != Some(0) {
            let result = ProgramResult {
                id: program.id.clone(),
                category: program.category.clone(),
                exit: outcome.status,
                expected_exit: program.expect_exit,
                skipped: false,
                reason: Some(if outcome.timed_out {
                    "setup timed out".to_string()
                } else {
                    format!("setup exited with {:?}", outcome.status)
                }),
                timed_out: outcome.timed_out,
                setup_failed: true,
                capture_error: None,
                duration_ms: elapsed_ms(started),
                syscalls_used: 0,
                strace: trace_path.display().to_string(),
                syscalls: BTreeMap::new(),
                unparsed_lines: 0,
            };
            write_result(out, &result)?;
            let _ = fs::remove_dir_all(&work);
            return Ok(result);
        }
    }

    let argv = program.expanded_argv(&work, corpus_dir);
    let mut args = vec![
        "-f".to_string(),
        "-qq".to_string(),
        "-s".to_string(),
        "64".to_string(),
        "-o".to_string(),
        trace_path.display().to_string(),
    ];
    args.extend(argv);
    let stdin_text = program.expanded_stdin(&work, corpus_dir);
    let outcome = match execute("strace", &args, &env, stdin_text.as_deref(), timeout, &work) {
        Ok(outcome) => outcome,
        Err(error) => {
            let result = ProgramResult {
                id: program.id.clone(),
                category: program.category.clone(),
                exit: None,
                expected_exit: program.expect_exit,
                skipped: false,
                reason: Some(format!("could not run strace: {error:#}")),
                timed_out: false,
                setup_failed: false,
                capture_error: Some(format!("could not run strace: {error:#}")),
                duration_ms: elapsed_ms(started),
                syscalls_used: 0,
                strace: trace_path.display().to_string(),
                syscalls: BTreeMap::new(),
                unparsed_lines: 0,
            };
            write_result(out, &result)?;
            let _ = fs::remove_dir_all(&work);
            return Ok(result);
        }
    };

    let trace = read_trace(&trace_path);
    let parsed = parse_trace(&trace.text);
    let result = ProgramResult {
        id: program.id.clone(),
        category: program.category.clone(),
        exit: outcome.status,
        expected_exit: program.expect_exit,
        skipped: false,
        reason: if outcome.timed_out {
            Some(format!("timed out after {} s", program.timeout_secs))
        } else if let Some(error) = &trace.error {
            Some(error.clone())
        } else if outcome.status != Some(program.expect_exit) {
            Some(format!(
                "exit {:?}, expected {}",
                outcome.status, program.expect_exit
            ))
        } else {
            None
        },
        timed_out: outcome.timed_out,
        setup_failed: false,
        capture_error: trace.error,
        duration_ms: elapsed_ms(started),
        syscalls_used: parsed.syscalls.len(),
        strace: trace_path.display().to_string(),
        syscalls: parsed.syscalls,
        unparsed_lines: parsed.unparsed_lines,
    };
    write_result(out, &result)?;
    let _ = fs::remove_dir_all(&work);
    Ok(result)
}

struct TraceInput {
    text: String,
    error: Option<String>,
}

fn read_trace(path: &Path) -> TraceInput {
    match fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => TraceInput { text, error: None },
            Err(error) => {
                let offset = error.utf8_error().valid_up_to();
                let text = String::from_utf8_lossy(error.as_bytes()).into_owned();
                TraceInput {
                    text,
                    error: Some(format!("trace is not valid UTF-8 at byte {offset}")),
                }
            }
        },
        Err(error) => TraceInput {
            text: String::new(),
            error: Some(format!("cannot read trace {}: {error}", path.display())),
        },
    }
}

fn write_result(out: &Path, result: &ProgramResult) -> Result<()> {
    let path = out.join(format!("{}.json", result.id));
    let mut json = serde_json::to_string_pretty(result).context("serialising program result")?;
    json.push('\n');
    fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn missing_requirement(requires: &[String]) -> Option<String> {
    for name in requires {
        if !command_exists(name) {
            return Some(name.clone());
        }
    }
    None
}

fn command_exists(name: &str) -> bool {
    if name.contains('/') {
        return Path::new(name).is_file();
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|directory| directory.join(name).is_file())
}

struct CommandOutcome {
    status: Option<i32>,
    timed_out: bool,
}

fn execute(
    program: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    stdin: Option<&str>,
    timeout: Duration,
    cwd: &Path,
) -> Result<CommandOutcome> {
    let mut command = Command::new(program);
    command
        .args(args)
        .envs(env)
        .current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let started = Instant::now();
    let mut child = command
        .spawn()
        .with_context(|| format!("starting `{program}`"))?;

    let (stdin_result, stdin_writer) = if let Some(text) = stdin {
        let mut pipe = child
            .stdin
            .take()
            .context("child stdin pipe was not created")?;
        let text = text.to_string();
        let (sender, receiver) = mpsc::channel();
        let writer = thread::spawn(move || {
            let _ = sender.send(pipe.write_all(text.as_bytes()));
        });
        (Some(receiver), Some(writer))
    } else {
        (None, None)
    };

    let mut timed_out = false;
    let status = loop {
        if let Some(exit) = child.try_wait()? {
            break exit.code();
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            kill_tree(&mut child);
            break child.wait()?.code();
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        thread::sleep(remaining.min(Duration::from_millis(10)));
    };

    if let Some(receiver) = stdin_result {
        let mut writer_done = match receiver.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => true,
            Err(TryRecvError::Empty) => false,
        };
        if !writer_done {
            // The child exited (or timed out) while a descendant still holds
            // the stdin read end. Kill the group so the writer cannot block
            // forever, then give it a bounded grace period.
            kill_tree(&mut child);
            writer_done = receiver.recv_timeout(Duration::from_secs(1)).is_ok();
        }
        if let Some(writer) = stdin_writer {
            if writer_done {
                let _ = writer.join();
            } else {
                drop(writer);
            }
        }
    } else if let Some(writer) = stdin_writer {
        let _ = writer.join();
    }

    Ok(CommandOutcome { status, timed_out })
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

/// Load the corpus and return the ids it contains (used by tests and CLI help).
pub fn program_ids(corpus_path: &Path) -> Result<Vec<String>> {
    let corpus = Corpus::load(corpus_path)?;
    Ok(corpus
        .programs
        .into_iter()
        .map(|program| program.id)
        .collect())
}

/// Ensure the output directory exists.
pub fn prepare_out(out: &Path) -> Result<()> {
    if out.exists() && !out.is_dir() {
        bail!("output path is not a directory: {}", out.display());
    }
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn corpus_text(id: &str) -> String {
        format!(
            "[[program]]\nid = \"{id}\"\ncategory = \"shell\"\n\
             argv = [\"sh\", \"-c\", \"echo hi\"]\n"
        )
    }

    #[test]
    fn missing_requirement_is_reported() {
        assert_eq!(
            missing_requirement(&["definitely-not-a-real-binary-b05".to_string()]),
            Some("definitely-not-a-real-binary-b05".to_string())
        );
        assert_eq!(missing_requirement(&["sh".to_string()]), None);
    }

    #[test]
    fn command_exists_uses_path() {
        assert!(command_exists("sh"));
        assert!(!command_exists("definitely-not-a-real-binary-b05"));
    }

    #[test]
    fn select_all_and_only() {
        let corpus =
            Corpus::parse(&format!("{}{}", corpus_text("one"), corpus_text("two"))).unwrap();
        assert_eq!(select_programs(&corpus, &[]).unwrap().len(), 2);
        assert_eq!(
            select_programs(&corpus, &["two".to_string()])
                .unwrap()
                .len(),
            1
        );
        assert!(select_programs(&corpus, &["nope".to_string()]).is_err());
    }

    #[test]
    fn run_one_program_writes_trace_and_json() {
        let temp = TempDir::new("runner-one");
        let corpus = temp.path().join("corpus.toml");
        temp.write("corpus.toml", &corpus_text("one"));
        let out = temp.path().join("out");
        let results = run_corpus(&RunOptions {
            corpus,
            out: out.clone(),
            only: vec!["one".to_string()],
        })
        .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());
        assert!(out.join("one.strace").is_file());
        let json = temp.read("out/one.json");
        let parsed: ProgramResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "one");
        assert_eq!(parsed.exit, Some(0));
    }

    #[test]
    fn skipped_program_writes_json_without_running() {
        let temp = TempDir::new("runner-skip");
        let corpus = temp.path().join("corpus.toml");
        temp.write(
            "corpus.toml",
            "[[program]]\nid = \"skip\"\ncategory = \"shell\"\nrequires = [\"definitely-not-a-real-binary-b05\"]\nargv = [\"true\"]\n",
        );
        let out = temp.path().join("out");
        let results = run_corpus(&RunOptions {
            corpus,
            out: out.clone(),
            only: Vec::new(),
        })
        .unwrap();
        assert!(results[0].skipped);
        assert_eq!(results[0].status(), "skipped");
        assert!(out.join("skip.json").is_file());
    }

    #[test]
    fn timeout_kills_a_sleeping_program() {
        let temp = TempDir::new("runner-timeout");
        let corpus = temp.path().join("corpus.toml");
        temp.write(
            "corpus.toml",
            "[[program]]\nid = \"slow\"\ncategory = \"shell\"\nargv = [\"sh\", \"-c\", \"sleep 5\"]\ntimeout_secs = 1\n",
        );
        let out = temp.path().join("out");
        let started = Instant::now();
        let results = run_corpus(&RunOptions {
            corpus,
            out,
            only: Vec::new(),
        })
        .unwrap();
        assert!(results[0].timed_out);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn setup_failure_is_reported() {
        let temp = TempDir::new("runner-setup");
        let corpus = temp.path().join("corpus.toml");
        temp.write(
            "corpus.toml",
            "[[program]]\nid = \"setupfail\"\ncategory = \"shell\"\nsetup = [[\"sh\", \"-c\", \"exit 4\"]]\nargv = [\"true\"]\n",
        );
        let out = temp.path().join("out");
        let results = run_corpus(&RunOptions {
            corpus,
            out,
            only: Vec::new(),
        })
        .unwrap();
        assert!(results[0].setup_failed);
        assert_eq!(results[0].status(), "setup-failed");
    }

    #[test]
    fn stdin_backpressure_respects_timeout_and_cleans_descendants() {
        let temp = TempDir::new("runner-stdin-timeout");
        let corpus = temp.path().join("corpus.toml");
        let text = format!(
            "[[program]]\nid = \"blocked-stdin\"\ncategory = \"shell\"\n\
             argv = [\"sh\", \"-c\", \"sleep 30 & echo $! > {{corpus}}/child.pid; wait\"]\n\
             timeout_secs = 1\nstdin = \"{}\"\n",
            "x".repeat(1024 * 1024)
        );
        temp.write("corpus.toml", &text);
        let out = temp.path().join("out");
        let started = Instant::now();
        let results = run_corpus(&RunOptions {
            corpus,
            out,
            only: Vec::new(),
        })
        .unwrap();
        assert!(results[0].timed_out, "{:?}", results[0]);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "took {:?}",
            started.elapsed()
        );

        let pid = temp.read("child.pid");
        let pid: u32 = pid.trim().parse().expect("valid descendant pid");
        let mut alive = true;
        for _ in 0..40 {
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
        assert!(!alive, "descendant process {pid} survived the timeout");
    }

    #[test]
    fn read_trace_missing_records_capture_error() {
        let temp = TempDir::new("runner-trace-missing");
        let input = read_trace(&temp.path().join("missing.strace"));
        assert!(input.error.is_some());
        assert!(input.text.is_empty());
    }

    #[test]
    fn read_trace_directory_records_capture_error() {
        let temp = TempDir::new("runner-trace-dir");
        let input = read_trace(temp.path());
        assert!(input.error.is_some());
    }

    #[test]
    fn read_trace_malformed_records_error_and_lossy_text() {
        let temp = TempDir::new("runner-trace-malformed");
        let path = temp.path().join("bad.strace");
        fs::write(&path, [0xffu8, 0xfe, b'\n']).unwrap();
        let input = read_trace(&path);
        assert!(input.error.as_deref().unwrap().contains("UTF-8"));
        let parsed = parse_trace(&input.text);
        assert!(parsed.unparsed_lines >= 1);
    }

    #[test]
    fn read_trace_empty_is_a_valid_empty_trace() {
        let temp = TempDir::new("runner-trace-empty");
        temp.write("empty.strace", "");
        let input = read_trace(&temp.path().join("empty.strace"));
        assert!(input.error.is_none());
        assert!(input.text.is_empty());
        assert_eq!(parse_trace(&input.text).unparsed_lines, 0);
    }
}
