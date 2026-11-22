// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only audit of the merged K332 RACK state and K333 RACK pipe groups.
//!
//! The audit is independent of the group reports: it checks the donor-name
//! inventory, the named tests, the `tcp_rack.h`/`tcp_var.h` constants and one
//! donor-arithmetic vector against the merged Rust. It reports exact findings;
//! it never edits product code.

#![forbid(unsafe_code)]

mod audit;

pub use audit::{
    AuditReport, DONOR_CONSTANTS, EXTRA_FUNCTIONS, Finding, Kind, PIPE_FUNCTIONS, STATE_FUNCTIONS,
    Sources, audit, audit_sources, fin_wait_vector, live_sources, tlp_gap_vector,
};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
