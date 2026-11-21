// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only independent model for RACK BBR bandwidth, pacing, timely and probe-RTT state.
//! This crate is an oracle for the future full RACK/BBR implementation; it never edits product
//! source and shares only the documented trace schema with the K319/K320 models.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

pub mod model;
pub mod rng;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
