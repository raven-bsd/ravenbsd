// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process execution for ATF test programs and exit-status programs.
//!
//! Every case gets a fresh working directory, a cleared environment, and its own process group so
//! a timeout kills grandchildren too. A timed-out run is judged by the result file: only an
//! `expected_timeout` directive turns it into a pass.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::atf_proto;
use crate::result::{judge, judge_missing, judge_unparsable, parse_result};
use crate::{ProcessStatus, TpCase, Verdict};

/// Largest result file the runner accepts; anything larger is truncated for parsing and reported.
const RESULT_LIMIT: u64 = 64 * 1024;
/// Largest stdout/stderr prefix kept for a report.
const OUTPUT_LIMIT: usize = 8 * 1024;
/// Poll interval while waiting for a child.
const POLL: Duration = Duration::from_millis(20);

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Options for running a test case.
#[derive(Clone, Debug)]
pub struct RunOptions {
    /// Maximum wall-clock seconds for one case.
    pub timeout_secs: u64,
    /// ATF configuration variables (`-v k=v`).
    pub vars: BTreeMap<String, String>,
    /// Keep the work directory after the case.
    pub keep: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout_secs: 300,
            vars: BTreeMap::new(),
            keep: false,
        }
    }
}

/// What one case observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    /// Verdict class.
    pub verdict: Verdict,
    /// Reason with a class token; empty only for pass and explicit skips handled by the caller.
    pub reason: String,
}

impl Observation {
    fn pass() -> Self {
        Self {
            verdict: Verdict::Pass,
            reason: String::new(),
        }
    }

    fn skip(reason: impl Into<String>) -> Self {
        Self {
            verdict: Verdict::Skip,
            reason: reason.into(),
        }
    }

    fn fail(reason: impl Into<String>) -> Self {
        Self {
            verdict: Verdict::Fail,
            reason: reason.into(),
        }
    }
}

/// Result of running a program with `-l`.
pub fn list(program: &Path, source_dir: Option<&Path>, timeout_secs: u64) -> Result<Vec<TpCase>> {
    let work = temporary_directory("list")?;
    let output = execute(
        program,
        &["-l".to_string()],
        &work,
        source_dir,
        timeout_secs,
    )
    .with_context(|| format!("cannot list {}", program.display()))?;
    let _ = fs::remove_dir_all(&work);
    if output.timed_out {
        bail!(
            "listing {} timed out after {timeout_secs}s",
            program.display()
        );
    }
    if let ProcessStatus::Signaled(signal) = output.status {
        bail!(
            "listing {} was killed by signal {signal}",
            program.display()
        );
    }
    if !matches!(output.status, ProcessStatus::Exited(0)) {
        bail!(
            "listing {} failed: {}",
            program.display(),
            first_line(&output.stderr)
        );
    }
    atf_proto::parse_list(&String::from_utf8_lossy(&output.stdout))
        .map_err(|error| anyhow::anyhow!("{}: {error}", program.display()))
}

/// Run one ATF test case.
pub fn run_case(
    program: &Path,
    case: &TpCase,
    source_dir: Option<&Path>,
    work_root: &Path,
    options: &RunOptions,
) -> Result<Observation> {
    if let Some(reason) = requirement_skip(case, options) {
        return Ok(Observation::skip(reason));
    }
    let timeout = case_timeout(case, options.timeout_secs);
    let work = fresh_directory(work_root, &case.ident)?;
    let shadow = prepare_srcdir(&work, program, source_dir)?;
    let result_path = work.join("result");
    let mut args = vec![
        "-r".to_string(),
        result_path.display().to_string(),
        "-s".to_string(),
        shadow.display().to_string(),
    ];
    for (key, value) in &options.vars {
        args.push("-v".to_string());
        args.push(format!("{key}={value}"));
    }
    args.push(case.ident.clone());

    let output = execute(program, &args, &work, Some(&shadow), timeout)
        .with_context(|| format!("cannot run {} ({})", program.display(), case.ident))?;

    let observation = judge_output(&output, &result_path, timeout);
    if !options.keep {
        let _ = fs::remove_dir_all(&work);
    }
    Ok(observation)
}

/// Run one exit-status program.
pub fn run_exit_status(
    program: &Path,
    args: &[String],
    expect_exit: i32,
    work_root: &Path,
    options: &RunOptions,
) -> Result<Observation> {
    let work = fresh_directory(work_root, "run")?;
    let output = execute(program, args, &work, None, options.timeout_secs)
        .with_context(|| format!("cannot run {}", program.display()))?;
    if !options.keep {
        let _ = fs::remove_dir_all(&work);
    }
    if output.timed_out {
        return Ok(Observation::fail(format!(
            "timeout after {}s",
            options.timeout_secs
        )));
    }
    match output.status {
        ProcessStatus::Signaled(signal) => Ok(Observation::fail(format!("crash:{signal}"))),
        ProcessStatus::Exited(code) if code == expect_exit => Ok(Observation::pass()),
        ProcessStatus::Exited(code) => Ok(Observation::fail(format!(
            "behaviour:expected exit {expect_exit}, got {code}"
        ))),
    }
}

/// Compute the effective timeout: the metadata `timeout` capped by `--max-timeout`.
#[must_use]
pub fn case_timeout(case: &TpCase, cap_secs: u64) -> u64 {
    case.meta
        .get("timeout")
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(cap_secs, |value| value.min(cap_secs))
}

/// Check the `require.*` metadata and return a skip reason when the case cannot run here.
#[must_use]
pub fn requirement_skip(case: &TpCase, options: &RunOptions) -> Option<String> {
    if case
        .meta
        .get("require.user")
        .is_some_and(|value| value == "root")
        && !running_as_root()
    {
        return Some("needs-root".to_string());
    }
    if let Some(programs) = case.meta.get("require.progs") {
        for program in programs.split_whitespace() {
            if find_in_path(program).is_none() {
                return Some(format!("needs-program:{program}"));
            }
        }
    }
    if let Some(files) = case.meta.get("require.files") {
        for file in files.split_whitespace() {
            if !Path::new(file).exists() {
                return Some(format!("needs-file:{file}"));
            }
        }
    }
    if let Some(machine) = case.meta.get("require.machine") {
        let allowed = host_machine_names();
        if !machine
            .split_whitespace()
            .any(|value| allowed.contains(&value))
        {
            return Some(format!("needs-machine:{machine}"));
        }
    }
    if let Some(arch) = case.meta.get("require.arch") {
        let allowed = host_arch_names();
        if !arch
            .split_whitespace()
            .any(|value| allowed.contains(&value))
        {
            return Some(format!("needs-arch:{arch}"));
        }
    }
    if let Some(config) = case.meta.get("require.config") {
        for key in config.split_whitespace() {
            if !options.vars.contains_key(key) {
                return Some(format!("needs-config:{key}"));
            }
        }
    }
    None
}

fn judge_output(output: &Completed, result_path: &Path, timeout_secs: u64) -> Observation {
    let bytes = match fs::read(result_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let (verdict, reason) = judge_missing(output.status, output.timed_out, timeout_secs);
            return Observation { verdict, reason };
        }
        Err(error) => {
            return Observation::fail(format!("build-other:cannot read result file: {error}"));
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    match parse_result(&text) {
        Ok(expect) => {
            let (verdict, reason) = judge(&expect, output.status, output.timed_out, timeout_secs);
            Observation { verdict, reason }
        }
        Err(_) => {
            let (verdict, reason) = judge_unparsable(&bytes, output.status);
            Observation { verdict, reason }
        }
    }
}

struct Completed {
    status: ProcessStatus,
    timed_out: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn execute(
    program: &Path,
    args: &[String],
    workdir: &Path,
    _source_dir: Option<&Path>,
    timeout_secs: u64,
) -> Result<Completed> {
    let stdout_path = workdir.join(".stdout");
    let stderr_path = workdir.join(".stderr");
    let stdout = fs::File::create(&stdout_path)
        .with_context(|| format!("cannot create {}", stdout_path.display()))?;
    let stderr = fs::File::create(&stderr_path)
        .with_context(|| format!("cannot create {}", stderr_path.display()))?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(workdir)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .env_clear()
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
        )
        .env("HOME", workdir)
        .env("TZ", "UTC")
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("TMPDIR", workdir);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("cannot spawn {}", program.display()))?;
    let pid = child.id();
    let start = Instant::now();
    let deadline = Duration::from_secs(timeout_secs);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().context("cannot poll a test process")? {
            break status;
        }
        if start.elapsed() >= deadline {
            timed_out = true;
            kill_group(&mut child, pid);
            break child
                .wait()
                .context("cannot reap a timed-out test process")?;
        }
        std::thread::sleep(POLL);
    };
    let code = status.code();
    let status = match code {
        Some(code) => ProcessStatus::Exited(code),
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                ProcessStatus::Signaled(status.signal().unwrap_or(0))
            }
            #[cfg(not(unix))]
            {
                ProcessStatus::Signaled(0)
            }
        }
    };
    Ok(Completed {
        status,
        timed_out,
        stdout: read_prefix(&stdout_path),
        stderr: read_prefix(&stderr_path),
    })
}

#[cfg(unix)]
fn kill_group(child: &mut Child, pid: u32) {
    let killed = Command::new("kill")
        .arg("-KILL")
        .arg("--")
        .arg(format!("-{pid}"))
        .status()
        .is_ok_and(|status| status.success());
    if !killed {
        let _ = child.kill();
    }
}

#[cfg(not(unix))]
fn kill_group(child: &mut Child, _pid: u32) {
    let _ = child.kill();
}

/// Make a fresh directory for one case under `root`.
fn fresh_directory(root: &Path, name: &str) -> Result<PathBuf> {
    let safe = sanitize(name);
    let path = root.join(safe);
    if path.exists() {
        fs::remove_dir_all(&path).with_context(|| format!("cannot clear {}", path.display()))?;
    }
    fs::create_dir_all(&path).with_context(|| format!("cannot create {}", path.display()))?;
    Ok(path)
}

fn temporary_directory(tag: &str) -> Result<PathBuf> {
    let root = std::env::temp_dir().join(format!(
        "raven-atf-run-{tag}-{}-{}",
        std::process::id(),
        next_id()
    ));
    fs::create_dir_all(&root).with_context(|| format!("cannot create {}", root.display()))?;
    Ok(root)
}

fn next_id() -> u64 {
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Build a shadow source directory containing the program and every data entry of `source_dir`.
///
/// ATF insists that the program executable exist in `-s <srcdir>`; donor tests also read data
/// files from it. Symlinks give both without touching the vendored tree.
fn prepare_srcdir(work: &Path, program: &Path, source_dir: Option<&Path>) -> Result<PathBuf> {
    let shadow = work.join("srcdir");
    fs::create_dir_all(&shadow).with_context(|| format!("cannot create {}", shadow.display()))?;
    if let Some(source) = source_dir
        && source.is_dir()
    {
        for entry in
            fs::read_dir(source).with_context(|| format!("cannot read {}", source.display()))?
        {
            let entry = entry.with_context(|| format!("cannot read {}", source.display()))?;
            let name = entry.file_name();
            let target = shadow.join(&name);
            let _ = std::os::unix::fs::symlink(entry.path(), target);
        }
    }
    let name = program
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("program path has no file name: {}", program.display()))?;
    let link = shadow.join(name);
    if link.exists() {
        fs::remove_file(&link).with_context(|| format!("cannot replace {}", link.display()))?;
    }
    std::os::unix::fs::symlink(program, &link).with_context(|| {
        format!(
            "cannot link {} into {}",
            program.display(),
            shadow.display()
        )
    })?;
    Ok(shadow)
}

fn read_prefix(path: &Path) -> Vec<u8> {
    let Ok(mut file) = fs::File::open(path) else {
        return Vec::new();
    };
    let mut buffer = Vec::new();
    let _ = file.by_ref().take(RESULT_LIMIT).read_to_end(&mut buffer);
    buffer.truncate(OUTPUT_LIMIT);
    buffer
}

fn first_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .next()
        .unwrap_or("")
        .to_string()
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        return Path::new(program).exists().then(|| PathBuf::from(program));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

fn running_as_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "0")
}

fn host_machine_names() -> Vec<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => vec!["amd64", "x86_64"],
        "x86" => vec!["i386", "x86"],
        "aarch64" => vec!["aarch64", "arm64", "evbarm"],
        "riscv64" => vec!["riscv64"],
        other => vec![other],
    }
}

fn host_arch_names() -> Vec<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => vec!["x86_64", "amd64"],
        "x86" => vec!["i386", "x86"],
        "aarch64" => vec!["aarch64", "arm64"],
        "riscv64" => vec!["riscv64"],
        other => vec![other],
    }
}

#[cfg(test)]
#[path = "runner_ut.rs"]
mod tests;
