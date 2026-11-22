// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side shell for the K491 rlimit fixture.
//!
//! The privilege rules are exercised on the target by `bin/init.rs`; this library keeps the
//! package's host tests separate from the kernel fixture.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

/// `RLIMIT_CPU`.
pub const RESOURCE_CPU: usize = 0;
/// `RLIMIT_NOFILE`.
pub const RESOURCE_NOFILE: usize = 7;
/// Number of resources in the kernel table.
pub const RESOURCE_COUNT: usize = 16;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
