// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-audit`: the target-case symbol audit as a build-system command.
//!
//! C64j owns the matcher in `scripts/libc-symbol-audit.sh`; this module is the `cargo xtask`
//! face of it. The C74 gate row runs `cargo xtask libc-audit` and treats exit 1 as a missing
//! required symbol. The script rebuilds `libc.so.1` when the export file is absent, so the
//! command works from a clean checkout.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, bail};

use crate::process::{CommandSpec, run as run_command};

const SCRIPT: &str = "scripts/libc-symbol-audit.sh";

const USAGE: &str = "\
usage: cargo xtask libc-audit [--rebuild] [--self-test]
                              [--exports FILE] [--required FILE]
                              [--expected FILE] [--out FILE]

Run the target-case libc symbol audit. Exit 0 when every required symbol is
exported; 1 when one is missing (the matrix is still written); 2 on usage,
I/O or a missing host tool.

  --rebuild     pack and link libc before auditing
  --self-test   run the fake-table matcher test, no libc or compiler needed
  --exports     dynamic export list to inspect (default build/x86_64/libc.so.1)
  --required    required-symbol TSV (default tests/libc/symbol-audit/required.tsv)
  --expected    expected matrix to compare (default tests/libc/symbol-audit/expected.tsv)
  --out         matrix output path (default build/libc-symbol-audit/matrix.tsv)";

/// Parsed `libc-audit` arguments.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Options {
    /// Pass `--rebuild`.
    pub(crate) rebuild: bool,
    /// Pass `--self-test`.
    pub(crate) self_test: bool,
    /// `--exports` value.
    pub(crate) exports: Option<String>,
    /// `--required` value.
    pub(crate) required: Option<String>,
    /// `--expected` value.
    pub(crate) expected: Option<String>,
    /// `--out` value.
    pub(crate) out: Option<String>,
}

impl Options {
    /// The exact argument vector for `scripts/libc-symbol-audit.sh`.
    #[must_use]
    pub(crate) fn script_args(&self) -> Vec<String> {
        let mut args = vec![SCRIPT.to_string()];
        if self.rebuild {
            args.push("--rebuild".to_string());
        }
        if self.self_test {
            args.push("--self-test".to_string());
        }
        for (flag, value) in [
            ("--exports", &self.exports),
            ("--required", &self.required),
            ("--expected", &self.expected),
            ("--out", &self.out),
        ] {
            if let Some(value) = value {
                args.push(flag.to_string());
                args.push(value.clone());
            }
        }
        args
    }
}

/// Parse the arguments after `libc-audit`.
pub(crate) fn parse(args: &[String]) -> Result<Options> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--rebuild" => options.rebuild = true,
            "--self-test" => options.self_test = true,
            "--exports" => {
                index += 1;
                options.exports = Some(value(args, index, "--exports")?);
            }
            "--required" => {
                index += 1;
                options.required = Some(value(args, index, "--required")?);
            }
            "--expected" => {
                index += 1;
                options.expected = Some(value(args, index, "--expected")?);
            }
            "--out" => {
                index += 1;
                options.out = Some(value(args, index, "--out")?);
            }
            other => bail!("unknown libc-audit option `{other}`"),
        }
        index += 1;
    }
    Ok(options)
}

fn value(args: &[String], index: usize, flag: &str) -> Result<String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
}

/// The child process for one parsed argument set.
#[must_use]
pub(crate) fn command(root: &Path, options: &Options) -> CommandSpec {
    CommandSpec::new("sh", options.script_args(), root)
}

/// Run `libc-audit`.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return Ok(0);
    }
    let options = parse(args)?;
    let code = run_command(&command(root, &options))?;
    Ok(u8::try_from(code).unwrap_or(1))
}

/// Run the command line and map errors to exit 2.
pub(crate) fn run_cli(args: &[String]) -> ExitCode {
    match root().and_then(|root| run(&root, args)) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn root() -> Result<PathBuf> {
    let current =
        std::env::current_dir().map_err(|error| anyhow::anyhow!("cannot get cwd: {error}"))?;
    crate::repo::find_root(&current)
}

#[cfg(test)]
#[path = "libc_audit_ut.rs"]
mod tests;
