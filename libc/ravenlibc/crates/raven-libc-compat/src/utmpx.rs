// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `utmpx` stubs (donor-map 15 T4, 02 P52, doc 15 4.6).
//!
//! Raven has no `utmp` database; sessions live in `procd`. `setutxent`/`endutxent` succeed,
//! every `getutx*` returns `None` (the shell sets `ENOENT`), `pututxline` returns its argument
//! and stores nothing, and `updwtmpx` is a no-op. The behaviour was read from
//! `musl:src/legacy/utmpx.c` (52 lines, MIT via `musl/COPYRIGHT`; STUDY, not copied).

/// `setutxent`: succeeds and does nothing.
pub fn setutxent() {}

/// `endutxent`: succeeds and does nothing.
pub fn endutxent() {}

/// `getutxent`: always `None`; the shell sets `ENOENT`.
#[must_use]
pub fn getutxent() -> Option<()> {
    None
}

/// `getutxid`: always `None`.
#[must_use]
pub fn getutxid(_id: &[u8]) -> Option<()> {
    None
}

/// `getutxline`: always `None`.
#[must_use]
pub fn getutxline(_line: &[u8]) -> Option<()> {
    None
}

/// `pututxline`: returns its argument and stores nothing (the shell returns the pointer).
#[must_use]
pub fn pututxline(line: &[u8]) -> &[u8] {
    line
}

/// `updwtmpx`: a no-op.
pub fn updwtmpx(_file: &[u8], _line: &[u8]) {}

/// The ticket's `pututxline_is_noop` probe: `true`, nothing is stored.
#[must_use]
pub fn pututxline_is_noop() -> bool {
    true
}

#[cfg(test)]
#[path = "utmpx_ut.rs"]
mod tests;
