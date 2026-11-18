// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-world` forwarder (C72).

use std::path::Path;

use anyhow::Result;

use crate::process::{CommandSpec, run as run_process};

const USAGE: &str = "\
usage:
  libc-world list    [--world F] [--tier N] [--subset NAME]
  libc-world check   [--world F] [--tarballs DIR] [--strict]
  libc-world build   (<name>... | --subset NAME | --tier N | --all) [flags]
  libc-world report  [--results DIR] [--arch A]
  libc-world missing [--results DIR] [--arch A] [--symbols F] [--resources DIR]
  libc-world gate    [--results DIR] [--arch A] [--phase 1|2] [--min-built N]
see tools/libc-world/README.md";

/// Whether the command is a help request (exit 2, like the C34 stub).
pub(crate) fn wants_usage(args: &[String]) -> bool {
    args.is_empty()
        || args
            .first()
            .is_some_and(|arg| arg == "--help" || arg == "-h")
}

/// The cargo argument vector that forwards to the tool.
pub(crate) fn forward_args(args: &[String]) -> Vec<String> {
    let mut vector = vec![
        "run".to_string(),
        "-q".to_string(),
        "-p".to_string(),
        "libc-world".to_string(),
        "--".to_string(),
    ];
    vector.extend(args.iter().cloned());
    vector
}

/// Convert the child exit code to the xtask return code.
pub(crate) fn exit_code(code: i32) -> u8 {
    u8::try_from(code).unwrap_or(2)
}

pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    if wants_usage(args) {
        println!("{USAGE}");
        return Ok(2);
    }
    let spec = CommandSpec::new("cargo", forward_args(args), root);
    Ok(exit_code(run_process(&spec)?))
}

#[cfg(test)]
#[path = "libc_world_ut.rs"]
mod tests;
