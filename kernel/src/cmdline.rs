// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel command-line parsing.
//!
//! The parser source is shared with the host-testable `raven-cmdline` crate without adding a
//! dependency to the kernel manifest (which is outside this ticket's Owns paths).

#[path = "../../crates/raven-cmdline/src/imp.rs"]
#[allow(dead_code)] // Shared crate source includes helpers used by its own tests.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The shared source is audited and linted in raven-cmdline's own host build.
mod imp;

pub use imp::{CmdLine, LogLevel};
