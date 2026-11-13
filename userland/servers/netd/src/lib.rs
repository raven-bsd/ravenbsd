// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `netd`: the `net:` socket scheme over the native socket core.
//!
//! The crate is split into a host-testable [`Netd`] state machine and target-only scheme/process
//! glue. Loopback (`127.0.0.1`) is always available; the target attaches the persistent `nic:`
//! frame service as a second core. IPv6 is not configured in this milestone.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod loopback;
mod netd;
#[cfg(target_os = "none")]
pub mod nic;

pub use loopback::Loopback;
pub use netd::{Interface, NetEvent, Netd, NicConfig};
pub use raven_proto_net::socket::Endpoint;
