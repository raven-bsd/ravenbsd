// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! OS-independent, backend-agnostic nonblocking socket layer.
//!
//! [`Core`] owns a [`StackBackend`] and a socket-id table. [`NetCore`] keeps the B07 name as the
//! smoltcp-backed alias. Every operation is nonblocking: `send`/`recv` return [`Errno::EAGAIN`]
//! when a buffer is empty or full, and [`Core::poll`] drives the backend and returns readiness
//! *transitions* since the previous call. No smoltcp type is re-exported from this crate.
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
pub use backend::smoltcp::SmoltcpBackend;
pub use core::Core;

pub use raven_abi::errno::Errno;

/// The smoltcp-backed core, preserving the B07 name.
pub type NetCore<D> = Core<SmoltcpBackend<D>>;

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
