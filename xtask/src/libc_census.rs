// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-census` forwarder (C33).

use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    let manifest = root.join("tools/libc-census/Cargo.toml");
    if !manifest.is_file() {
        bail!("libc-census: not implemented by C34 (ticket C33)");
    }
    let status = Command::new("cargo")
        .args(["run", "-q", "-p", "libc-census", "--"])
        .args(args)
        .current_dir(root)
        .status()?;
    Ok(status.code().unwrap_or(1).try_into().unwrap_or(1))
}
