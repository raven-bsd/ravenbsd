// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! OS-independent nonblocking socket layer over the native `raven-tcpip` stack.
//!
//! [`Core`] owns a [`StackBackend`]. [`NetCore`] is the native-backed core and the only one
//! shipped; the trait stays as the seam for stubs and future transports. Every operation is
//! nonblocking: `send`/`recv` return [`Errno::EAGAIN`] when a buffer is empty or full, and
//! [`Core::poll`] drives the backend and returns readiness *transitions* since the previous
//! call. A link is any [`Device`] (re-exported from `raven-tcpip`).
//!
//! The crate is `no_std` and needs only `alloc`. Tests and host harnesses use the in-memory
//! `testutil::PairDevice` link.

#![no_std]
#![forbid(unsafe_code)]
#![allow(missing_docs)] // Linux/UAPI-style operation names are documented at module level.

extern crate alloc;

mod address;
mod backend;
mod core;

pub use address::Ipv4Addr;
pub use backend::StackBackend;
pub use backend::raven::RavenTcpipBackend;
pub use core::Core;

pub use raven_abi::errno::Errno;
pub use raven_tcpip::{ChecksumOffload, Device, DeviceCaps, Instant, Medium, TxError};

/// The native raven-tcpip-backed core.
pub type NetCore<D> = Core<RavenTcpipBackend<D>>;

/// Socket identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SocketId(pub u32);

/// Readiness transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Readable.
    Readable(SocketId),
    /// Writable.
    Writable(SocketId),
    /// Closed.
    Closed(SocketId),
    /// Error.
    Error(SocketId, Errno),
    /// Accepted connection.
    Accepted { listener: SocketId, conn: SocketId },
}

#[cfg(any(test, feature = "testutil"))]
pub mod testutil;
