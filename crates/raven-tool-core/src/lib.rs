// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure helpers shared by the base file tools.
//!
//! The crate is host-testable and has no syscalls: option parsing, error lines, mode strings,
//! size formatting and path joins live here. The target programs combine these helpers with
//! `raven_rt::fs`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod error;
pub mod format;
pub mod opts;
pub mod path;

pub use crate::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE};
pub use crate::opts::{Flags, Parsed, UsageError, parse};
pub use raven_abi::errno::Errno;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
