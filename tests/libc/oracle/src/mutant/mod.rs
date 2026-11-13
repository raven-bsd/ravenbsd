// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deliberately broken variants of the stand-ins.
//!
//! The names are API for the mutation tests: each variant must be killed by the differential
//! harness, both against the Rust stand-in and against the C oracle.

pub mod memchr;
pub mod strlcpy;
pub mod strnlen;

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
