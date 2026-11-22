// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! First-party HTTP/1.1 GET client core for RavenBSD.
//!
//! The crate is a pure, host-testable model of the request side of HTTP/1.1: a request builder,
//! a status/header parser, `Content-Length`, chunked and close-delimited bodies, a bounded
//! redirect policy and a total timeout. Sockets, DNS and TLS stay outside: the caller injects a
//! connected [`Transport`] and a monotonic [`Clock`], so the network service owns
//! `resolvd`/`net:` and the E-Ink service owns caching and refresh.
//!
//! The core speaks plain HTTP only. HTTPS is deliberately not implemented here: DR-45 keeps TLS
//! in the owner-approved OpenSSL add-on process, and a pure-Rust TLS stack would add an
//! unaudited crypto implementation or C/assembly to the C-free base. See
//! `docs/design/eink-fetch.md` for the decision and the E3b add-on boundary.
//!
//! Every input from the peer is bounded: status line, header block, header count, chunk line and
//! body size. A malformed or truncated response is an [`Error`], never a panic.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod client;
mod header;
mod request;
mod response;
mod url;

pub use client::Client;
pub use header::Header;
pub use request::{Request, RequestBuilder};
pub use response::Response;
pub use url::{Scheme, Url};

/// Default TCP port for plain HTTP.
pub const DEFAULT_PORT: u16 = 80;
/// Default total request timeout in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u32 = 10_000;
/// Default maximum number of followed redirects.
pub const DEFAULT_MAX_REDIRECTS: u8 = 5;
/// Default maximum response body accepted from one request.
pub const DEFAULT_MAX_BODY: usize = 1 << 20;
/// Maximum status-line bytes accepted from a peer.
pub const MAX_STATUS_LINE: usize = 512;
/// Maximum bytes in one header line.
pub const MAX_HEADER_LINE: usize = 8 * 1024;
/// Maximum bytes across the whole response header block.
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
/// Maximum number of response headers.
pub const MAX_HEADERS: usize = 64;
/// Maximum bytes in one chunk-size line.
pub const MAX_CHUNK_LINE: usize = 128;
/// Maximum URL length accepted by the request builder.
pub const MAX_URL: usize = 2048;

/// A transport-layer failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    /// The transport deadline expired.
    Timeout,
    /// The connection failed or the peer closed it unexpectedly.
    Io,
}

/// One connected byte stream.
///
/// The implementation owns the real `resolvd`/`net:` or TLS pipe. The core calls [`Self::connect`]
/// once per request and once per redirect target, then uses [`Self::write`] and [`Self::read`].
/// A read returning `Ok(0)` is an orderly close, which is how a close-delimited body ends. Every
/// call receives the milliseconds left in the request's total timeout; the transport must honour
/// it (the core also checks the clock between calls).
pub trait Transport {
    /// Open a connection to `host:port` within `timeout_ms`.
    fn connect(&mut self, host: &str, port: u16, timeout_ms: u32) -> Result<(), TransportError>;

    /// Write `bytes`, returning the number accepted.
    fn write(&mut self, bytes: &[u8], timeout_ms: u32) -> Result<usize, TransportError>;

    /// Read up to `bytes.len()` bytes; `Ok(0)` means end of stream.
    fn read(&mut self, bytes: &mut [u8], timeout_ms: u32) -> Result<usize, TransportError>;
}

/// A monotonic millisecond clock used for the bounded total timeout.
pub trait Clock {
    /// Milliseconds from an arbitrary fixed origin.
    fn now_ms(&self) -> u64;
}

/// A request, response or transport failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The URL is malformed, too long or uses an unsupported scheme.
    InvalidUrl,
    /// The request fields are malformed or contain a control character.
    InvalidRequest,
    /// The peer response is not valid HTTP/1.x.
    InvalidResponse,
    /// A status line or header block exceeded its bound.
    HeaderLimit,
    /// The body exceeded the caller's bound or `Content-Length` overflowed.
    BodyLimit,
    /// The total timeout expired.
    Timeout,
    /// The transport failed.
    Transport,
    /// The core only speaks plain HTTP; `https` needs the TLS add-on process.
    UnsupportedScheme,
    /// The redirect budget was exhausted.
    TooManyRedirects,
    /// A redirect status carried no `Location`.
    MissingLocation,
    /// The peer closed before the declared body was complete.
    Truncated,
    /// A chunked body is malformed.
    Chunked,
    /// A length or arithmetic conversion overflowed.
    Overflow,
}
