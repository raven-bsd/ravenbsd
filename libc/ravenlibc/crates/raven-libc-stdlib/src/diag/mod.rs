// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Diagnostics: `assert`, the `v*` `err`/`warn` writers, the program-name store and
//! `<libgen.h>`.

pub mod assert;
pub mod basename;
pub mod dirname;
pub mod err_core;
pub mod libgen;
pub mod progname;
pub mod verr;
pub mod verrc;
pub mod verrx;
pub mod vwarn;
pub mod vwarnc;
pub mod vwarnx;

pub use assert::{MESSAGE_CAPACITY, assert_bsd, assert_fail, assert_message, assert2};
pub use basename::basename;
pub use dirname::dirname;
pub use err_core::DiagOut;
pub use libgen::GenName;
pub use progname::ProgName;
pub use verr::verr;
pub use verrc::verrc;
pub use verrx::verrx;
pub use vwarn::vwarn;
pub use vwarnc::vwarnc;
pub use vwarnx::vwarnx;

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
