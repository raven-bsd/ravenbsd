// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host regressions for the C76 spawn descriptor and eventfd paths.
//!
//! The target proofs are `libc-spawn-attr` and `brush-spawn`; these tests pin the portable
//! contracts underneath them: the RFDM block round-trips and restores a usable descriptor,
//! `write(2)`-level buffers keep embedded NULs, and the eventfd counter/readiness engine used
//! by `read(2)`/`write(2)` behaves as an eventfd.

#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
