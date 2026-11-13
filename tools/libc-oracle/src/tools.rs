// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tool discovery (ticket C35, section 4.4).
//!
//! `cc`, `ld`, `nm` and `objcopy` are taken from `$CC`, `$LD`, `$NM` and `$OBJCOPY` when set and
//! from `PATH` otherwise. `objcopy` falls back to `llvm-objcopy`. Every tool is probed with
//! `--version`; a missing or failing probe is exit 77 (`skipped: <tool> not found`).

use std::process::{Command, Stdio};

use anyhow::Result;

use crate::failure::Failure;

/// The four host tools a build needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tools {
    /// C compiler.
    pub(crate) cc: String,
    /// Linker used for `ld -r`.
    pub(crate) ld: String,
    /// Symbol lister.
    pub(crate) nm: String,
    /// Symbol renamer.
    pub(crate) objcopy: String,
}

/// Discover and probe the host tools from the process environment.
pub(crate) fn discover() -> Result<Tools> {
    discover_with(&|name| std::env::var(name).ok(), &|program| {
        Command::new(program)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// Discovery with an explicit environment lookup and probe, for tests.
pub(crate) fn discover_with(
    lookup: &dyn Fn(&str) -> Option<String>,
    probe: &dyn Fn(&str) -> bool,
) -> Result<Tools> {
    let cc = lookup("CC").unwrap_or_else(|| "cc".to_string());
    let ld = lookup("LD").unwrap_or_else(|| "ld".to_string());
    let nm = lookup("NM").unwrap_or_else(|| "nm".to_string());
    if !probe(&cc) {
        return Err(Failure::skipped("cc"));
    }
    if !probe(&ld) {
        return Err(Failure::skipped("ld"));
    }
    if !probe(&nm) {
        return Err(Failure::skipped("nm"));
    }
    let objcopy = if let Some(explicit) = lookup("OBJCOPY") {
        if !probe(&explicit) {
            return Err(Failure::skipped("objcopy"));
        }
        explicit
    } else if probe("objcopy") {
        "objcopy".to_string()
    } else if probe("llvm-objcopy") {
        "llvm-objcopy".to_string()
    } else {
        return Err(Failure::skipped("objcopy"));
    };
    Ok(Tools {
        cc,
        ld,
        nm,
        objcopy,
    })
}

#[cfg(test)]
#[path = "tools_ut.rs"]
mod tests;
