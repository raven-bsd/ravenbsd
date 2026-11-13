// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Wire protocol for the `net:` scheme server and its NIC driver rings.
//!
//! `netd` is the only consumer of a NIC driver's rings, and both processes need one versioned,
//! golden-tested contract before either exists (`architecture/03-system-servers.md` section 2,
//! `architecture/07-network.md`). This crate defines:
//!
//! * [`socket`] - `OPEN`, `READ`, `WRITE` and typed `CALL` operations for a `net:` socket;
//! * [`iface`] - interface, route and ARP control on the `net:` control channel;
//! * [`path`] - parsing of `net:tcp/<addr>:<port>`, `net:udp/...`, `net:icmp/<addr>` and `net:`;
//! * [`ring`] - the shared-memory header, descriptor rings and buffer pool between `netd` and a
//!   driver, plus the single-producer/single-consumer helpers.
//!
//! The crate is `no_std`, has no dependency except `raven-abi`, forbids `unsafe`, never allocates
//! in an encoder and never panics while decoding. Every message starts with a little-endian `u16`
//! version; an unknown version is rejected with [`DecodeError`], not reinterpreted. Every length
//! is checked against a documented bound before use. See `README.md` for the byte tables that
//! match the golden tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod iface;
pub mod path;
pub mod ring;
pub mod socket;
pub mod wire;

use core::fmt;

pub use iface::{
    ARP_COMPLETE, ARP_STATIC, ArpEntry, CounterInfo, DHCP_TIMER_UNKNOWN, DhcpLeaseInfo, IF_LINK_UP,
    IF_LOOPBACK, IF_UP, IfName, IfaceInfo, IfaceRequest, IfaceResponse, MAX_DHCP_DNS, MAX_IFNAME,
    ROUTE_FLAG_MASK, ROUTE_GATEWAY, ROUTE_HOST, ROUTE_UP, Route, RouteInfo, SOCKET_ICMP,
    SOCKET_STATE_CLOSE_WAIT, SOCKET_STATE_CLOSED, SOCKET_STATE_CLOSING, SOCKET_STATE_ESTABLISHED,
    SOCKET_STATE_FIN_WAIT_1, SOCKET_STATE_FIN_WAIT_2, SOCKET_STATE_ICMP, SOCKET_STATE_LAST_ACK,
    SOCKET_STATE_LISTEN, SOCKET_STATE_SYN_RECEIVED, SOCKET_STATE_SYN_SENT, SOCKET_STATE_TIME_WAIT,
    SOCKET_STATE_UDP, SOCKET_TCP, SOCKET_UDP, SocketEntry,
};
pub use path::{ACCEPT_PREFIX, PathError, SchemePath, Transport, parse_accepted};
pub use raven_abi::errno::Errno;
pub use ring::{
    BUFFER_SIZE, DESC_CHECKSUM_OK, DESC_ERROR, DESC_FIRST, DESC_FLAG_MASK, DESC_LAST, Desc,
    LayoutError, MAX_REGION, MAX_RING_ENTRIES, RING_MAGIC, RING_VERSION, Ring, RingConsumer,
    RingError, RingHeader, RingLayout, RingProducer,
};
pub use socket::{
    Call, Endpoint, MAX_BACKLOG, MAX_DATAGRAM, MAX_ICMP_PAYLOAD, MAX_ICMP_TIMEOUT_MS, MAX_PATH,
    Response, Shutdown, SockOpt, SocketRequest,
};
pub use wire::{Reader, WireError, Writer};

/// Version of the socket, interface and driver-ring framing.
pub const PROTOCOL_VERSION: u16 = socket::PROTOCOL_VERSION;

/// Why a message was rejected by a decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeErrorKind {
    /// The input ended before the message did.
    Truncated,
    /// The input continues after the message ended.
    TrailingBytes,
    /// The first field is a version this crate does not speak.
    UnsupportedVersion,
    /// An operation code, response tag or option code is unknown.
    UnknownTag,
    /// A length or count is outside its documented bound.
    LengthOutOfRange,
    /// A field is well formed but violates an invariant (reserved bits, bad UTF-8, ...).
    Malformed,
}

/// A decoder failure. Decoders return this instead of panicking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError {
    kind: DecodeErrorKind,
}

impl DecodeError {
    /// Build an error of `kind`.
    pub const fn new(kind: DecodeErrorKind) -> Self {
        Self { kind }
    }

    /// Classify the failure.
    pub const fn kind(self) -> DecodeErrorKind {
        self.kind
    }

    /// The input ended too early.
    pub const fn truncated() -> Self {
        Self::new(DecodeErrorKind::Truncated)
    }

    /// The input has bytes after the message.
    pub const fn trailing() -> Self {
        Self::new(DecodeErrorKind::TrailingBytes)
    }

    /// The message uses another protocol version.
    pub const fn version() -> Self {
        Self::new(DecodeErrorKind::UnsupportedVersion)
    }

    /// An operation, tag or option code is unknown.
    pub const fn tag() -> Self {
        Self::new(DecodeErrorKind::UnknownTag)
    }

    /// A length or count is out of range.
    pub const fn length() -> Self {
        Self::new(DecodeErrorKind::LengthOutOfRange)
    }

    /// A field violates a protocol invariant.
    pub const fn malformed() -> Self {
        Self::new(DecodeErrorKind::Malformed)
    }
}

impl From<WireError> for DecodeError {
    fn from(error: WireError) -> Self {
        match error {
            WireError::Truncated => Self::truncated(),
            WireError::TooLong => Self::length(),
            WireError::Invalid => Self::malformed(),
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            DecodeErrorKind::Truncated => "truncated message",
            DecodeErrorKind::TrailingBytes => "trailing bytes after the message",
            DecodeErrorKind::UnsupportedVersion => "unsupported protocol version",
            DecodeErrorKind::UnknownTag => "unknown operation, tag or option",
            DecodeErrorKind::LengthOutOfRange => "length or count out of range",
            DecodeErrorKind::Malformed => "malformed field",
        })
    }
}
