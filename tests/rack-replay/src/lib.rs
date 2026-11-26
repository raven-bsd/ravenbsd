// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only RACK full-port replay and group-report union audit.
//!
//! This crate is evidence tooling, not product code. It replays the independent K319/K320/
//! K324/K326/K327 model traces against the ported `Rack`, `SackScoreboard`, `Tlp` and
//! `RackStack` primitives and merges the K269/K328-K336 coverage reports through the K315
//! checker. A red trace is a finding for the owning group, not a test weakening.

#![forbid(unsafe_code)]

pub mod replay;
pub mod trace;
pub mod union;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
