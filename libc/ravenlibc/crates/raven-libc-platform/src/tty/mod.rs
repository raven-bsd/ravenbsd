// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Terminal helpers.
//!
//! `isatty` is a bionic COPY (row 5); the termios family is a clean-room dispatch over the tty
//! `CALL` protocol, which today has `SET_MODE`, `GET_MODE` and `FLUSH` only (PG-54 adds the rest).

pub mod isatty;

pub use isatty::isatty;

/// `TTY_NAME_MAX` (doc 11 section 3.2).
pub const TTY_NAME_MAX: usize = 32;

#[cfg(test)]
#[path = "tty_ut.rs"]
mod tests;
