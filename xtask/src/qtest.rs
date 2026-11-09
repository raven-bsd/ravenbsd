// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Forwarding to the `raven-qtest` harness (ticket B01).

use std::path::Path;

use anyhow::{Result, bail};

use crate::process::{CommandSpec, run as run_command};

/// Forward `args` to `cargo run -q -p raven-qtest -- <args>`.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    if !root.join("tests/qtest/Cargo.toml").is_file() {
        bail!("qtest: tests/qtest does not exist yet (ticket B01)");
    }
    let mut command_args = vec![
        "run".to_string(),
        "-q".to_string(),
        "-p".to_string(),
        "raven-qtest".to_string(),
        "--".to_string(),
    ];
    command_args.extend(args.iter().cloned());
    let code = run_command(&CommandSpec::new("cargo", command_args, root))?;
    Ok(u8::from(code != 0))
}
