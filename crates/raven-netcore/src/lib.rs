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

/// An address lease obtained over DHCP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DhcpLease {
    /// The leased address.
    pub address: Ipv4Addr,
    /// Prefix length of the connected network.
    pub prefix_len: u8,
    /// Default gateway, if the server gave one.
    pub router: Option<Ipv4Addr>,
    /// DNS servers.
    pub dns: alloc::vec::Vec<Ipv4Addr>,
    /// Milliseconds until renewal, when the backend exposes it.
    pub renew_in_ms: Option<u32>,
    /// Milliseconds until rebinding, when the backend exposes it.
    pub rebind_in_ms: Option<u32>,
    /// Milliseconds until expiry, when the backend exposes it.
    pub expires_in_ms: Option<u32>,
}

/// One IPv4 route in the native table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteEntry {
    /// Destination prefix.
    pub dest: Ipv4Addr,
    /// Destination prefix length.
    pub prefix_len: u8,
    /// Next hop, absent for an on-link route.
    pub gateway: Option<Ipv4Addr>,
    /// Path MTU.
    pub mtu: usize,
}

/// Transport of one live socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketTransport {
    /// TCP stream or listener.
    Tcp,
    /// UDP datagram.
    Udp,
    /// ICMP echo.
    Icmp,
}

/// State of one live socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketState {
    /// No connection.
    Closed,
    /// Listening.
    Listen,
    /// SYN sent.
    SynSent,
    /// SYN received.
    SynReceived,
    /// Established.
    Established,
    /// FIN wait 1.
    FinWait1,
    /// FIN wait 2.
    FinWait2,
    /// Close wait.
    CloseWait,
    /// Closing.
    Closing,
    /// Last ACK.
    LastAck,
    /// TIME-WAIT.
    TimeWait,
    /// Unconnected UDP.
    Udp,
    /// Bound ICMP echo.
    Icmp,
}

/// One live socket from the native core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreSocket {
    /// Core socket id.
    pub id: SocketId,
    /// Transport.
    pub transport: SocketTransport,
    /// Current state.
    pub state: SocketState,
    /// Local endpoint, when known.
    pub local: Option<(Ipv4Addr, u16)>,
    /// Remote endpoint, when connected.
    pub remote: Option<(Ipv4Addr, u16)>,
    /// Bytes queued for the application.
    pub rx_queue: u32,
    /// Bytes queued for the wire.
    pub tx_queue: u32,
}

/// Packet and byte counters observed at one device boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InterfaceCounters {
    /// Frames received.
    pub rx_packets: u64,
    /// Frames transmitted.
    pub tx_packets: u64,
    /// Bytes received.
    pub rx_bytes: u64,
    /// Bytes transmitted.
    pub tx_bytes: u64,
    /// Receive errors.
    pub rx_errors: u64,
    /// Transmit errors.
    pub tx_errors: u64,
    /// Frames dropped by the stack.
    pub rx_drops: u64,
    /// Frames dropped by the device.
    pub tx_drops: u64,
}

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
