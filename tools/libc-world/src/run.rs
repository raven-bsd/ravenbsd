// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process execution with an explicit environment, a timeout and a process group (section 4.3).

use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use std::os::unix::process::CommandExt as _;

/// A fully specified child process.
#[derive(Clone, Debug)]
pub struct RunSpec {
    /// Program name or absolute path.
    pub program: String,
    /// Argument vector.
    pub args: Vec<String>,
    /// Working directory.
    pub cwd: PathBuf,
    /// Explicit environment (applied after `env_clear`).
    pub env: Vec<(String, String)>,
    /// Per-step timeout in seconds.
    pub timeout_secs: u64,
    /// Combined stdout/stderr log path.
    pub log: PathBuf,
}

/// The result of one step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunOutput {
    /// Exit code (`None` for a signal or a timeout).
    pub exit: Option<i32>,
    /// The timeout fired.
    pub timed_out: bool,
    /// Combined log text.
    pub text: String,
}

impl RunSpec {
    /// A step with the default environment.
    #[must_use]
    pub fn new(
        program: impl Into<String>,
        args: Vec<String>,
        cwd: impl Into<PathBuf>,
        log: impl Into<PathBuf>,
        timeout_secs: u64,
    ) -> Self {
        Self {
            program: program.into(),
            args,
            cwd: cwd.into(),
            env: vec![(
                "PATH".to_string(),
                std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
            )],
            timeout_secs,
            log: log.into(),
        }
    }

    /// Add or replace an environment entry.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        self.env.retain(|(candidate, _)| candidate != &key);
        self.env.push((key, value.into()));
        self
    }
}

/// Run one step, capturing both streams into its log file.
pub fn run(spec: &RunSpec) -> Result<RunOutput> {
    if let Some(parent) = spec.log.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let file =
        File::create(&spec.log).with_context(|| format!("cannot create {}", spec.log.display()))?;
    let errors = file.try_clone()?;
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(errors))
        .process_group(0);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("cannot run `{}`", spec.program))?;
    let start = Instant::now();
    let timeout = Duration::from_secs(spec.timeout_secs.max(1));
    let (exit, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status.code(), false);
        }
        if start.elapsed() >= timeout {
            terminate(&mut child);
            let status = child.wait()?;
            break (status.code(), true);
        }
        thread::sleep(Duration::from_millis(20));
    };
    let text = std::fs::read_to_string(&spec.log).unwrap_or_default();
    Ok(RunOutput {
        exit,
        timed_out,
        text,
    })
}

fn terminate(child: &mut std::process::Child) {
    let group = format!("-{}", child.id());
    // The `--` matters: without it procps `kill` parses `-<pid>` as another option and hangs.
    let sent = Command::new("kill")
        .arg("-TERM")
        .arg("--")
        .arg(&group)
        .status()
        .is_ok_and(|status| status.success());
    if !sent {
        let _ = child.kill();
        return;
    }
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = Command::new("kill")
        .arg("-KILL")
        .arg("--")
        .arg(&group)
        .status();
}

#[cfg(test)]
#[path = "run_ut.rs"]
mod tests;
