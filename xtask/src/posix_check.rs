// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `cargo xtask posix-check`: run the published POSIX self-certification gate.

use std::path::Path;

use anyhow::Result;

use crate::process::{CommandSpec, run as run_command};

/// Forward the gate flags to `posix-conform gate`.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    let mut command_args = vec![
        "run".to_string(),
        "-q".to_string(),
        "-p".to_string(),
        "posix-conform".to_string(),
        "--".to_string(),
        "gate".to_string(),
    ];
    command_args.extend(args.iter().cloned());
    let spec = CommandSpec::new("cargo", command_args, root);
    let code = run_command(&spec)?;
    Ok(u8::from(code != 0))
}
