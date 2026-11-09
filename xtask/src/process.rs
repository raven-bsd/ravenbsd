// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command execution with an explicit environment.
//!
//! Every child process is spawned with an argument vector (never through a shell) and an explicit
//! working directory. Reproducibility variables and component build flags are set here so the
//! caller's environment cannot leak into a build.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

/// A fully specified child process.
#[derive(Clone, Debug)]
pub(crate) struct CommandSpec {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) env_remove: Vec<String>,
    pub(crate) cwd: PathBuf,
}

impl CommandSpec {
    pub(crate) fn new(
        program: impl Into<String>,
        args: Vec<String>,
        cwd: impl Into<PathBuf>,
    ) -> Self {
        Self {
            program: program.into(),
            args,
            env: Vec::new(),
            env_remove: Vec::new(),
            cwd: cwd.into(),
        }
    }

    pub(crate) fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub(crate) fn env_remove(mut self, key: impl Into<String>) -> Self {
        self.env_remove.push(key.into());
        self
    }
}

/// Run `spec`, returning its exit code (`1` when the process was killed by a signal).
pub(crate) fn run(spec: &CommandSpec) -> Result<i32> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args).current_dir(&spec.cwd);
    for key in &spec.env_remove {
        command.env_remove(key);
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    if verbose() {
        eprintln!(
            "+ (cd {} && {} {})",
            spec.cwd.display(),
            spec.program,
            spec.args.join(" ")
        );
    }
    let status = command
        .status()
        .with_context(|| format!("cannot run `{}`", spec.program))?;
    Ok(status.code().unwrap_or(1))
}

fn verbose() -> bool {
    std::env::var("VERBOSE").is_ok_and(|value| value == "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_spec_builder_keeps_arguments_in_order() {
        let spec = CommandSpec::new("cargo", vec!["build".into(), "--release".into()], "/tmp")
            .env("A", "1")
            .env_remove("B");
        assert_eq!(spec.args, vec!["build", "--release"]);
        assert_eq!(spec.env, vec![("A".to_string(), "1".to_string())]);
        assert_eq!(spec.env_remove, vec!["B"]);
    }
}
