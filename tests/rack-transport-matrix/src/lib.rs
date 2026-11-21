// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only matrix over the merged RACK transport/HPTS paths.
//!
//! This crate is evidence tooling: it calls every public `rack_transport` path, compares the
//! overlapping outcomes with the K320/K327 independent models and checks the K315 union row. It
//! never edits product source and copies no donor expression.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
