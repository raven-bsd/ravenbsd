// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only TCP receive-window, zero-window and persist model.
//!
//! This crate is test/evidence tooling, not product code. It drives the public
//! `Stack`/`SocketId` API through window shrink, persist probes, reopen/window updates,
//! SWS and duplicate ACKs, and compares the delivered byte stream against a reference
//! window model using an FNV-1a hash. Findings belong to K265/K266.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

mod harness;
mod model;
mod rng;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
