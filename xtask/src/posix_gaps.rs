// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `cargo xtask posix-gaps [--check]`: validate the POSIX profile and (re)generate GAPS.md.

use std::path::Path;

use anyhow::Result;

use crate::process::{CommandSpec, run as run_command};

/// Run the profile tool through `cargo run` so the tool remains a normal workspace member.
pub(crate) fn run(root: &Path, check: bool) -> Result<u8> {
    let command = if check { "check" } else { "generate" };
    let spec = CommandSpec::new(
        "cargo",
        vec![
            "run".to_string(),
            "-q".to_string(),
            "-p".to_string(),
            "posix-conform".to_string(),
            "--".to_string(),
            command.to_string(),
            "--root".to_string(),
            ".".to_string(),
        ],
        root,
    );
    let code = run_command(&spec)?;
    Ok(u8::from(code != 0))
}
