// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Error and signal messages: the message tables and the `strerror`/`strsignal` engines.
//!
//! The exported C functions live in the shell; the per-thread 64-byte buffer is a parameter here
//! (RD-13). Nothing in this module touches `errno`: the XSI return code is the whole contract
//! (A-str-7), and the caller maps [`Xsi`] to its return value.

pub mod strerror;
pub mod strerror_r;
pub mod strsignal;
pub mod tables;

pub use strerror::{message, strerror_into};
pub use strerror_r::{Xsi, strerror_r_xsi};
pub use strsignal::strsignal_into;

/// Size of the per-thread message buffer (RD-13 / A-str-6).
pub const BUF_LEN: usize = 64;

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
