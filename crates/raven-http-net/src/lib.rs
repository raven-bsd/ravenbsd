// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Platform socket and `resolvd` adapter for [`raven_http`].
//!
//! [`raven_http`] owns the HTTP state machine and sees only a [`raven_http::Transport`]. This
//! crate supplies that transport from two injectable pieces:
//!
//! * [`Socket`] — one connected TCP endpoint (`connect`/`send`/`recv`/`close`) with a bounded
//!   timeout for each operation.
//! * [`Resolver`] — one IPv4 lookup by host name.
//!
//! Host tests use mock implementations and fixed response bytes; the target implementation
//! ([`live_transport`]) uses the `net:` and `resolv:` schemes through `raven-scheme`, so the
//! network stack stays in `netd`/`resolvd` and this crate never opens a device or links C.
//! IPv4 is the 0.1.0 path. TLS is deliberately absent: the E3a decision keeps plain HTTP in the
//! base and routes HTTPS through the owner-approved OpenSSL add-on process (DR-45).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg_attr(not(any(test, target_os = "none")), allow(dead_code))]
mod resolv;
mod transport;

#[cfg(target_os = "none")]
mod scheme;

use core::net::Ipv4Addr;

pub use transport::HttpTransport;

#[cfg(target_os = "none")]
pub use scheme::{SchemeResolver, SchemeSocket, live_transport};

/// A socket operation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketError {
    /// The operation deadline expired before it completed.
    Timeout,
    /// The connection failed, was reset, or the peer closed a write.
    Io,
}

/// A name-resolution failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveError {
    /// The name has no IPv4 address.
    NotFound,
    /// The resolver deadline expired.
    Timeout,
    /// The resolver exchange failed.
    Io,
}

/// One connected TCP endpoint.
///
/// `connect` must replace any previous endpoint. `send` may accept fewer bytes than offered;
/// `recv` returns `Ok(0)` for an orderly peer close. Both receive the milliseconds left before
/// the caller's total deadline and must bound the operation accordingly; a peer that sends no
/// data until the deadline is `SocketError::Timeout`, not a busy loop.
pub trait Socket {
    /// Open a TCP connection to `address:port`.
    fn connect(&mut self, address: Ipv4Addr, port: u16, timeout_ms: u32)
    -> Result<(), SocketError>;

    /// Send `bytes`, returning the number accepted.
    fn send(&mut self, bytes: &[u8], timeout_ms: u32) -> Result<usize, SocketError>;

    /// Receive up to `bytes.len()` bytes; `Ok(0)` means an orderly close.
    fn recv(&mut self, bytes: &mut [u8], timeout_ms: u32) -> Result<usize, SocketError>;

    /// Close the current endpoint. Closing an unconnected socket is not an error.
    fn close(&mut self) -> Result<(), SocketError>;
}

/// One resolved IPv4 address and its DNS TTL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// The IPv4 address.
    pub address: Ipv4Addr,
    /// TTL in seconds; zero means the value is not cacheable (for example an IP literal).
    pub ttl_secs: u32,
}

/// Resolve one host name to an IPv4 address.
///
/// The real implementation asks `resolvd` over the `resolv:` scheme. `timeout_ms` is the
/// caller's remaining budget; the resolver service owns the DNS exchange timeout, so the
/// implementation passes the value through as far as the scheme allows and the service keeps an
/// outer deadline around the whole fetch. The returned TTL is policy input for the service cache.
pub trait Resolver {
    /// Resolve `host` to one IPv4 address and its TTL.
    fn resolve(&mut self, host: &str, timeout_ms: u32) -> Result<Resolved, ResolveError>;
}
