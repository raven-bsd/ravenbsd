// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only TCP input model and property suite.
//!
//! This crate is test/evidence tooling, not product code. It checks the K282 input path
//! independently of the product tests: a clean-room model of the RFC 9293 / RFC 5961 input
//! rules and a deterministic harness that drives the real `raven-tcpip` stack through the
//! same cases. Findings belong to K282/K291; this crate never edits product source.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

mod harness;
mod model;
mod rng;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
