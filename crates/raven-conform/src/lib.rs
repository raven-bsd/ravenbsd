// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Conformance result parsing and comparison.
//!
//! The crate converts pjdfstest TAP-like output and LTP logs into one JSON schema, enforces a
//! shrink-only `KNOWN_FAILURES` list, and prints per-suite Markdown summaries. The binary is
//! `raven-conform`; the parsers are also usable as a library.

#![forbid(unsafe_code)]

pub mod compare;
pub mod known;
pub mod ltp;
pub mod pjdfstest;
pub mod result;
pub mod summary;

pub use compare::{Comparison, compare};
pub use known::KnownFailures;
pub use result::{Results, Status, TestResult};
