// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `raven` command metadata, parser and host-side command modules.
//!
//! The library is deliberately thin: [`run`] parses the global contract, selects one
//! [`Command`] and hands it the remaining arguments. The target wiring and the server-backed
//! commands arrive in K78; the reserved names return `EUNAVAILABLE` today.

#![forbid(unsafe_code)]

mod cli;
mod cmd;
mod commands;

pub use cli::{Context, Io, run};
pub use commands::{COMMANDS, Command, CommandMeta};
pub use raven_output::Exit;

/// The client version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The contract target reported by `raven version`.
///
/// The host skeleton reports the RavenBSD triple for the architecture it was built for; the
/// target wiring replaces the build and boot identifiers, not the triple.
#[cfg(target_arch = "x86_64")]
pub const TARGET: &str = "x86_64-unknown-ravenbsd";

/// The contract target reported by `raven version` on aarch64.
#[cfg(target_arch = "aarch64")]
pub const TARGET: &str = "aarch64-unknown-ravenbsd";

/// The contract target reported by `raven version` on riscv64.
#[cfg(target_arch = "riscv64")]
pub const TARGET: &str = "riscv64-unknown-ravenbsd";

/// The fallback target for a host architecture without a RavenBSD triple.
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
pub const TARGET: &str = "unknown-unknown-ravenbsd";

/// Find one registered command by name.
#[must_use]
pub fn command(name: &str) -> Option<&'static dyn Command> {
    commands::command(name)
}
