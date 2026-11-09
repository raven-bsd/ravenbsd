// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD licence and provenance checker.
//!
//! Implements the rules in `architecture/12-licensing-and-provenance.md` and contract C11:
//! SPDX headers on first-party files, the `PROVENANCE.toml` import schema, donor tiers,
//! the donors lock, and the dependency licence allow-list.

#![forbid(unsafe_code)]

mod check;
mod deps;
mod discover;
mod ignore;
mod model;
mod provenance;
mod spdx;

#[cfg(test)]
mod testutil;

pub use check::{check, fix_spdx, list};
pub use model::{CheckOptions, CheckReport, Violation};
