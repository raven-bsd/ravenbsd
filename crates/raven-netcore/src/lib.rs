// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! OS-independent socket layer over smoltcp.
//!
//! [`NetCore`] owns a smoltcp interface, a socket set and a table of socket ids. Every operation
//! is nonblocking: `send`/`recv` return [`Errno::EAGAIN`] when a buffer is empty or full, and
//! [`NetCore::poll`] drives the stack and returns readiness *transitions* since the previous
//! call.
//!
//! The crate is `no_std` and needs only `alloc`. Tests and host harnesses use the in-memory
//! [`testutil::PairDevice`] link.

#![no_std]
#![forbid(unsafe_code)]
#![allow(missing_docs)] // Linux/UAPI-style operation names are documented at module level.

extern crate alloc;

mod netcore;

pub use netcore::{Event, NetCore, SocketId};

pub use raven_abi::errno::Errno;
pub use smoltcp::phy::Device;
pub use smoltcp::wire::Ipv4Address as Ipv4Addr;

#[cfg(any(test, feature = "testutil"))]
pub mod testutil;
