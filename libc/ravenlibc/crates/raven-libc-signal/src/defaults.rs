// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The default action of every signal (ticket C58 section 4.11, Linux numbering).
//!
//! One byte per signal 1..64; 32..34 are libc-internal and never user visible. `Core` means the
//! process ends with the signal status and the core flag in the wait status (C53 codec); no core
//! file is written in v1. The rows are the POSIX/Linux conventions musl follows; every row is
//! asserted by a table test.

/// What happens when a signal has `SIG_DFL`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DefaultAction {
    /// The process ends with the signal status.
    Terminate,
    /// The process ends with the signal status and the core flag.
    Core,
    /// The signal is discarded.
    Ignore,
    /// Stop the process (PG-10 absent; treated as ignore plus a debug line).
    Stop,
    /// Continue a stopped process.
    Continue,
}

/// Byte encoding used by the const table.
const T: u8 = 1;
const C: u8 = 2;
const I: u8 = 3;
const S: u8 = 4;
const G: u8 = 5;

/// Signals 0..=64; index 0 and 32..34 are zero (no user-visible default).
const ACTIONS: [u8; 65] = [
    0, 1, 1, 2, 2, 2, 2, 2, 2, 1, 1, 2, 1, 1, 1, 1, 1, 3, 5, 4, 4, 4, 4, 3, 2, 2, 1, 1, 3, 1, 1, 2,
    0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1,
];

/// The default action, or `None` for 0, 32..34 and numbers above 64.
#[must_use]
#[allow(clippy::indexing_slicing)] // the range is checked on the line above the index
pub const fn default_action(sig: i32) -> Option<DefaultAction> {
    if sig < 1 || sig > 64 {
        return None;
    }
    match ACTIONS[sig as usize] {
        T => Some(DefaultAction::Terminate),
        C => Some(DefaultAction::Core),
        I => Some(DefaultAction::Ignore),
        S => Some(DefaultAction::Stop),
        G => Some(DefaultAction::Continue),
        _ => None,
    }
}

#[cfg(test)]
#[path = "defaults_ut.rs"]
mod defaults_ut;
