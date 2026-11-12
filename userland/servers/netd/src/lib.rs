// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `netd`: the `net:` socket scheme over the native socket core.
//!
//! The crate is split into a host-testable [`Netd`] state machine and a target-only scheme/process
//! glue. The first milestone runs IPv4 loopback (`127.0.0.1`); the NIC-ring seam is documented in
//! `docs/man8/netd.8` and is wired by B12b. IPv6 is not configured in this milestone.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod loopback;
mod netd;

pub use loopback::Loopback;
pub use netd::Netd;
pub use raven_proto_net::socket::Endpoint;
