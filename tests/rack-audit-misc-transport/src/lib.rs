// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent audit of the merged K334/K335 RACK misc/transport group.
//!
//! Test-only; no product source is edited. The fixtures pin the donor function
//! list, donor constant values and the K315 manifest so the audit is
//! reproducible without a donor checkout.

#![forbid(unsafe_code)]

/// Pinned FreeBSD donor revision.
pub const DONOR_REV: &str = "133344405f532bd54ff3629a77d4c4e062f01fdb";
/// K334 rows in the group fixture.
pub const K334_ROWS: usize = 28;
/// K335 rows in the group fixture.
pub const K335_ROWS: usize = 9;
/// Unique donor functions after removing the known duplicate claim.
pub const UNIQUE_GROUP_FUNCTIONS: usize = 36;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
