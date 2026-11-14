// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `__error`/`__errno`: shell-only aliases of the errno cell (`02` B20).
//!
//! There is no logic here: the shell returns `__errno_location()` for both names. This module
//! records the contract and gives the export table one place to reference.

/// The C names that return the address of the calling thread's errno cell.
pub const ERRNO_ALIASES: &[&str] = &["__error", "__errno"];

#[cfg(test)]
#[path = "errno_alias_ut.rs"]
mod tests;
