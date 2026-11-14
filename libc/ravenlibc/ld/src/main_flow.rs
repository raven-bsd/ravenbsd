// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! The start-up flow (the host-testable part of `_rtld_main`).
//!
//! The real entry glue parses the stack and auxv; this module takes a [`StartCtx`] and executes
//! the steps that are the same on the host and the target.

use raven_libc_elf::{Arch, ObjId, Scope};

use crate::loadobj::{LoadError, Loader, Name};
use crate::mapper::Platform;

/// The start-up inputs.
#[derive(Clone, Copy, Debug)]
pub struct StartCtx<'a> {
    /// The main program path.
    pub main_path: &'a [u8],
    /// The requested main name.
    pub main_name: &'a [u8],
    /// Additional search directories.
    pub library_path: &'a [&'a [u8]],
    /// The architecture.
    pub arch: Arch,
    /// The trusted flag.
    pub trust: bool,
}

/// The result of start-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartResult {
    /// The program entry address.
    pub entry: usize,
    /// The exit function address (zero when none).
    pub exit_proc: usize,
}

/// Run the host-testable start-up steps.
pub fn rtld_main<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier>(
    loader: &mut Loader<P, M, A>,
    ctx: StartCtx<'_>,
) -> Result<StartResult, LoadError> {
    loader.arch = ctx.arch;
    loader.trust = ctx.trust;
    for dir in ctx.library_path {
        let _ = loader.add_library_path(dir);
    }
    let main = loader.load_object(ctx.main_path, ctx.main_name)?;
    loader.scope = Scope::new(main);
    loader.load_needed_objects(main)?;
    loader.relocate_all()?;
    let entry = loader
        .object(main)
        .map(|object| object.view.bias)
        .unwrap_or(0);
    Ok(StartResult {
        entry: usize::try_from(entry).unwrap_or(0),
        exit_proc: 0,
    })
}

/// Parse one `LD_LIBRARY_PATH` value into a search list (no allocation).
pub fn split_paths<'a>(value: &'a [u8], out: &'a mut [Name]) -> usize {
    let mut count = 0usize;
    for part in value.split(|byte| *byte == b':') {
        if count >= out.len() {
            break;
        }
        if let Some(name) = Name::new(part) {
            out[count] = name;
            count = count.saturating_add(1);
        }
    }
    count
}

/// The main object id after start-up.
#[must_use]
pub fn main_id() -> ObjId {
    ObjId(0)
}

#[cfg(test)]
#[path = "main_flow_ut.rs"]
mod tests;
