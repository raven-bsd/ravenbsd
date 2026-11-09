// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux syscall corpus runner and coverage dashboard.
//!
//! The tool runs a curated list of Linux programs under `strace -f`, parses the
//! traces, and merges the results into `docs/linux-syscall-coverage.{json,md}`.
//! It is a host tool; it never runs as root and never touches the network
//! except for the corpus entries that explicitly use loopback.

#![forbid(unsafe_code)]

pub mod corpus;
pub mod merge;
pub mod runner;
pub mod strace;
pub mod syscalls;

#[cfg(test)]
mod testutil;
