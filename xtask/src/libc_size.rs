// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-size` forwarder (C69 4.2).
//!
//! `cargo xtask libc-size [ARGS]` forwards verbatim to `cargo run -q -p libc-size -- ARGS`.
//! `cargo xtask libc-size --check` is the `17-crate-layout.md` section 14 shorthand: run the
//! matrix, then check it against the committed budgets.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

/// The child command of one invocation.
#[must_use]
pub fn command_for(args: &[String]) -> Vec<String> {
    let mut command = vec![
        "run".to_string(),
        "-q".to_string(),
        "-p".to_string(),
        "libc-size".to_string(),
        "--".to_string(),
    ];
    command.extend(args.iter().cloned());
    command
}

/// Split `--check` from the forwarded run flags.
#[must_use]
pub fn split_check(args: &[String]) -> (Vec<String>, bool) {
    let check = args.iter().any(|arg| arg == "--check");
    let forwarded = args
        .iter()
        .filter(|arg| arg.as_str() != "--check")
        .cloned()
        .collect();
    (forwarded, check)
}

/// Insert the implicit `run` command when only flags were given
/// (`cargo xtask libc-size --check` is the 17-crate-layout section 14 shorthand).
#[must_use]
pub fn forward_args(forwarded: &[String]) -> Vec<String> {
    if forwarded.is_empty() || forwarded.first().is_some_and(|arg| arg.starts_with("--")) {
        let mut args = vec!["run".to_string()];
        args.extend(forwarded.iter().cloned());
        return args;
    }
    forwarded.to_vec()
}

/// Run one `cargo run -q -p libc-size -- ...` child.
fn execute(root: &Path, args: &[String]) -> Result<u8> {
    let status = Command::new("cargo")
        .args(command_for(args))
        .current_dir(root)
        .status()
        .context("cannot run cargo for libc-size")?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(2))
}

/// `cargo xtask libc-size ...`.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    let (forwarded, check_after) = split_check(args);
    let run_args = forward_args(&forwarded);
    let code = execute(root, &run_args)?;
    if code != 0 {
        return Ok(code);
    }
    if check_after {
        return execute(root, &["check".to_string()]);
    }
    Ok(0)
}

#[cfg(test)]
#[path = "libc_size_ut.rs"]
mod tests;
