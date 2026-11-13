// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The six platform seams and the environment bundle.
//!
//! The resolver never calls a system primitive: files, sockets, the clock, entropy, route
//! probing and interface names all arrive through these traits, so the whole crate runs on the
//! host with fakes.

use alloc::string::String;
use alloc::vec::Vec;

use raven_libc_nss::FileSource;

use crate::types::{Family, Interest, IpAddr, Recv, SockAddr, TcpSock, UdpSock};

/// The UDP/TCP socket operations the engine needs.
pub trait DnsIo {
    /// Open a fresh UDP socket bound to a random ephemeral port.
    ///
    /// The engine never binds a fixed port and never reuses a socket between lookups (A-net-2).
    fn open_udp(&self, family: Family) -> Result<UdpSock, crate::Errno>;

    /// Send one datagram; returns the byte count.
    fn send_to(&self, s: UdpSock, buf: &[u8], to: &SockAddr) -> Result<usize, crate::Errno>;

    /// Receive one datagram; `Err(EAGAIN)` when the socket is empty.
    fn recv_from(&self, s: UdpSock, buf: &mut [u8]) -> Result<Recv, crate::Errno>;

    /// Start a non-blocking connect to `to`.
    fn open_tcp(&self, to: &SockAddr) -> Result<TcpSock, crate::Errno>;

    /// Send bytes on a TCP socket; partial writes are allowed.
    fn tcp_send(&self, s: TcpSock, buf: &[u8]) -> Result<usize, crate::Errno>;

    /// Receive bytes from a TCP socket; `0` means EOF.
    fn tcp_recv(&self, s: TcpSock, buf: &mut [u8]) -> Result<usize, crate::Errno>;

    /// Wait for readiness and set `ready_r`/`ready_w` on the entries that fired.
    fn wait(&self, interests: &mut [Interest], timeout_ms: u64) -> Result<(), crate::Errno>;

    /// Close a UDP socket.
    fn close_udp(&self, s: UdpSock);

    /// Close a TCP socket.
    fn close_tcp(&self, s: TcpSock);
}

/// The monotonic clock seam.
pub trait Clock {
    /// Milliseconds since an arbitrary monotonic origin.
    fn monotonic_ms(&self) -> u64;
}

/// The query-ID entropy seam.
///
/// Returning `false` means no ID is available; the lookup then fails with `EAI_FAIL`. There is
/// never a clock or counter fallback (T-12-6).
pub trait Entropy {
    /// Fill `buf` with random bytes.
    fn fill(&self, buf: &mut [u8]) -> bool;
}

/// The route and family probes.
///
/// Both may say no (A-net-9): no usable route for a destination, or no usable family.
pub trait Probe {
    /// The local source address a UDP socket would use for `dst`, or `None` when unroutable.
    fn udp_route(&self, dst: &SockAddr) -> Option<IpAddr>;
    /// Whether a UDP socket for `f` can be created and connected to the loopback.
    fn family_usable(&self, f: Family) -> bool;
}

/// Interface index and name lookups.
pub trait IfNames {
    /// The index for an interface name.
    fn name_to_index(&self, name: &str) -> Option<u32>;
    /// The name for an interface index.
    fn index_to_name(&self, idx: u32) -> Option<String>;
}

/// Everything the resolver may touch outside its own logic.
pub struct Env<'a> {
    /// `/etc` file access.
    pub files: &'a dyn FileSource,
    /// Socket operations.
    pub io: &'a dyn DnsIo,
    /// The monotonic clock.
    pub clock: &'a dyn Clock,
    /// Query-ID entropy.
    pub entropy: &'a dyn Entropy,
    /// Route and family probes.
    pub probe: &'a dyn Probe,
    /// Interface names.
    pub ifnames: &'a dyn IfNames,
}

impl<'a> Env<'a> {
    /// Read a file through the file seam.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, crate::Errno> {
        self.files.read_file(path, crate::consts::FILE_READ_MAX)
    }

    /// Fill `buf` from the entropy seam.
    #[must_use]
    pub fn random(&self, buf: &mut [u8]) -> bool {
        self.entropy.fill(buf)
    }
}

#[cfg(test)]
#[path = "traits_ut.rs"]
mod tests;
