// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic TCP timer conformance (K292).
//!
//! Drives the K267 FreeBSD timer state machine with explicit virtual milliseconds:
//! `tcp_timer_rexmt` (RTO/backoff), `tcp_timer_keep` (keepalive) and `tcp_timer_2msl`
//! (TIME-WAIT). The packet-level RTO scenario remains the K253/K275 wire harness.

#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
