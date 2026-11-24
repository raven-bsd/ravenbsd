// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only TCP timestamps/PAWS and sequence-wrap model.
//!
//! This is test/evidence tooling, not product code. It drives the public
//! `Stack`/`SocketId` API through timestamp negotiation, PAWS accept/reject, the RFC 7323
//! 24-day exception, sequence wrap, old duplicates, zero-window timestamp probes and a
//! 10,000-event randomized sequence. The delivered byte stream is hashed and compared with
//! the independent reference model. Findings belong to the TCP owners; no product source is
//! edited.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

mod harness;
mod model;
mod rng;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
