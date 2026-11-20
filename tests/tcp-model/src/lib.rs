// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only TCP model and property tests.
//!
//! This crate is a test oracle, not product code. It ports the permissive FreeBSD
//! `sys/netinet/tcp_seq.h` model and re-derives the state, window, RTO and SACK references from
//! RFC 9293, 6298 and 6675. Divergences are reported, not patched here.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

mod congestion;
mod rng;
mod rto;
mod sack;
mod seq;
mod state;
mod window;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
