// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only randomized ECN, PMTU and PLPMTUD model.
//!
//! This crate is a test oracle, not product code. It re-derives the reference state machines from
//! the merged FreeBSD-aligned `raven-tcpip` behaviour and RFC 3168/8899/1191, then drives the
//! public `Ecn`, `EcnState`, `MtuCache`, PMTU/MSS helpers and `Plpmtud` through fixed-seed random
//! sequences. Divergences are reported to K270, not patched here.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), allow(dead_code))]

mod ecn;
mod plpmtud;
mod pmtu;
mod rng;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
