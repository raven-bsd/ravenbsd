// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic simulated link and trace recorder for the TCP/IP stack.
//!
//! This crate is host-only test tooling. It never uses wall-clock time, threads or real sockets:
//! every delay is virtual milliseconds and every random decision comes from the caller's seed.

#![forbid(unsafe_code)]

mod link;
mod sim;
mod trace;

pub use link::{Endpoint, Link, LinkConfig};
pub use sim::{Sim, Timeout};
pub use trace::{Direction, Mismatch, Trace, TraceEntry};

pub use raven_tcpip::{Duration, Instant};
