// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only matrix over the merged K336 RACK SACK/DSACK/reneging module.
//!
//! This crate is evidence tooling: it calls every public `RackSack` path, compares the result
//! with an independent range reference, replays the K319/K324 trace seeds and checks the K315
//! group rows. It does not edit product source.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

pub mod reference;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
