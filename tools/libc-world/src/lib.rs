// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Build-the-world harness for ravenlibc (C72).
//!
//! The crate unpacks owner-supplied pristine tarballs, builds them with a generated wrapper
//! compiler against the ravenlibc sysroot, and records symbol/header/library evidence in the
//! exact input format of the C33 census. It never fetches anything and never runs target code.

#![forbid(unsafe_code)]

pub mod builders;
pub mod census_out;
pub mod classify;
pub mod extract;
pub mod gate;
pub mod model;
pub mod patches;
pub mod report;
pub mod run;
pub mod sha256;
pub mod tarball;
pub mod testutil;
pub mod world;
pub mod wrapper;

/// Schema version written by this tool.
pub const SCHEMA: u32 = 1;

/// Default world list, relative to the repository root.
pub const DEFAULT_WORLD: &str = "libc/ravenlibc/world.toml";

/// Default committed results directory, relative to the repository root.
pub const DEFAULT_RESULTS: &str = "tools/libc-world/results";

/// Default build directory, relative to the repository root.
pub const DEFAULT_OUT: &str = "build/libc-world";

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
