// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only audit of the merged RACK output/timer and ACK/retransmit groups.
//!
//! Reads the product sources and unit tests, checks the K330/K331 function/test/mutation table and
//! the donor constants, and records findings. It never edits product code; findings go to the
//! group owners.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

pub mod audit;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
