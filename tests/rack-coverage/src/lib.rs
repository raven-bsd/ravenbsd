// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only RACK donor-function coverage checker.
//!
//! Parses the pinned FreeBSD `sys/netinet/tcp_stacks/rack.c` function definitions and validates
//! that `docs/rack-coverage.md` (or the K269 report) names every one exactly once with a Rust
//! mapping, a named test, a mutation/negative check and `PASS`. This crate never edits product
//! source; it is acceptance tooling for K269.

#![forbid(unsafe_code)]

pub mod donor;
pub mod matrix;

pub use donor::{Function, parse_functions, parse_manifest};
pub use matrix::{Report, Row, check, parse_matrix};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
