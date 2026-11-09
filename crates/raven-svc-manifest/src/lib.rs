// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Strict, allocation-based service manifest parser and dependency graph.
//!
//! `init` describes its services with small TOML manifests (CONTRACTS K10 and
//! `architecture/03-system-servers.md` section 3). This crate implements exactly the documented
//! TOML subset, the `rc.conf`-style overrides and the start-order computation. It is `no_std` +
//! `alloc`, dependency-free and contains no `unsafe` code, so PID 1 can use it before a libc or a
//! filesystem exists.
//!
//! The parser is deliberately strict: unknown keys, duplicate keys, wrong value types and values
//! outside the documented limits are errors with a 1-based line number. The graph rejects missing
//! dependencies, ambiguous scheme providers and cycles; a successful [`Plan`] is deterministic.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod graph;
pub mod manifest;
pub mod overrides;
mod parse;

pub use graph::{GraphError, Plan, start_order};
pub use manifest::{Manifest, ParseError, ParseErrorKind, Readiness, Restart};
pub use overrides::{Override, Overrides, UnknownService, apply_overrides, parse_overrides};
