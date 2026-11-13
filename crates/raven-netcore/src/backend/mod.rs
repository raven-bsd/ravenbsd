// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The backend seam: the socket core knows only this trait.

use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::{Event, Ipv4Addr, SocketId};

mod counting;
pub mod raven;

/// A replaceable TCP/IP stack behind the nonblocking socket core.
///
/// The trait is deliberately shaped around socket operations, not around any
/// one stack's interface/socket-set types. `Device` is the backend's own
/// device type; the core never names it.
pub trait StackBackend {
    /// The backend's device type.
    type Device;

    /// Configure a static IPv4 address and optional gateway.
    fn set_ipv4(&mut self, addr: Ipv4Addr, prefix: u8, gateway: Option<Ipv4Addr>);

    /// Enable DHCP.
    fn enable_dhcp(&mut self);

    /// Enable DHCP, or report that this backend cannot.
    ///
    /// The default forwards to [`Self::enable_dhcp`] and succeeds; a backend without a client
    /// overrides it to return [`Errno::EOPNOTSUPP`].
    fn try_enable_dhcp(&mut self) -> Result<(), Errno> {
        self.enable_dhcp();
        Ok(())
    }

    /// The DHCP lease in force, once one was acquired.
    fn dhcp_lease(&self) -> Option<crate::DhcpLease> {
        None
    }

    /// IPv4 routes in the native table.
    fn routes(&self) -> Vec<crate::RouteEntry> {
        Vec::new()
    }

    /// Add or replace one route.
    fn add_route(&mut self, route: crate::RouteEntry) -> Result<(), Errno> {
        let _ = route;
        Err(Errno::EOPNOTSUPP)
    }

    /// Delete one route by destination prefix.
    fn delete_route(&mut self, dest: Ipv4Addr, prefix_len: u8) -> Result<(), Errno> {
        let _ = (dest, prefix_len);
        Err(Errno::EOPNOTSUPP)
    }

    /// Live sockets in the native core.
    fn socket_table(&self) -> Vec<crate::CoreSocket> {
        Vec::new()
    }

    /// Packet/byte counters observed at the device boundary.
    fn interface_counters(&self) -> Option<crate::InterfaceCounters> {
        None
    }

    /// Bind one ICMP echo identifier.
    fn icmp_bind(&mut self, id: u16) -> Result<SocketId, Errno> {
        let _ = id;
        Err(Errno::EOPNOTSUPP)
    }

    /// Send one ICMP echo request on a bound ICMP socket.
    fn icmp_echo(
        &mut self,
        s: SocketId,
        to: Ipv4Addr,
        id: u16,
        seq: u16,
        payload: &[u8],
    ) -> Result<(), Errno> {
        let _ = (s, to, id, seq, payload);
        Err(Errno::EOPNOTSUPP)
    }

    /// Read one ICMP echo reply payload.
    fn icmp_recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno> {
        let _ = (s, buf);
        Err(Errno::EOPNOTSUPP)
    }

    /// Listen on `port`.
    fn tcp_listen(&mut self, port: u16, backlog: usize) -> Result<SocketId, Errno>;

    /// Accept one completed connection.
    fn tcp_accept(&mut self, listener: SocketId) -> Result<Option<SocketId>, Errno>;

    /// Start a connection.
    fn tcp_connect(
        &mut self,
        remote: (Ipv4Addr, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno>;

    /// Queue TCP bytes.
    fn send(&mut self, s: SocketId, data: &[u8]) -> Result<usize, Errno>;

    /// Read TCP bytes.
    fn recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno>;

    /// Shut down the write side.
    fn shutdown_write(&mut self, s: SocketId) -> Result<(), Errno>;

    /// Close a socket.
    fn close(&mut self, s: SocketId) -> Result<(), Errno>;

    /// Bind a UDP socket.
    fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno>;

    /// Send a UDP datagram.
    fn udp_send_to(
        &mut self,
        s: SocketId,
        data: &[u8],
        to: (Ipv4Addr, u16),
    ) -> Result<usize, Errno>;

    /// Receive a UDP datagram.
    fn udp_recv_from(
        &mut self,
        s: SocketId,
        buf: &mut [u8],
    ) -> Result<(usize, (Ipv4Addr, u16)), Errno>;

    /// Drive the backend and return readiness transitions.
    fn poll(&mut self, now_ms: i64) -> Vec<Event>;

    /// Time until the next poll, in milliseconds.
    fn next_poll_delay_ms(&mut self, now_ms: i64) -> Option<u64>;

    /// Local endpoint of `s`, when the backend exposes one.
    fn local_endpoint(&self, s: SocketId) -> Option<(Ipv4Addr, u16)> {
        let _ = s;
        None
    }

    /// Peer endpoint of `s`, when connected.
    fn peer_endpoint(&self, s: SocketId) -> Option<(Ipv4Addr, u16)> {
        let _ = s;
        None
    }

    /// Apply a protocol socket option; the backend owns any unsupported-option policy.
    fn set_socket_option(&mut self, s: SocketId, option: u32, value: i32) -> Result<(), Errno> {
        let _ = (s, option, value);
        Err(Errno::EOPNOTSUPP)
    }

    /// Read a protocol socket option.
    fn get_socket_option(&self, s: SocketId, option: u32) -> Result<i32, Errno> {
        let _ = (s, option);
        Err(Errno::EOPNOTSUPP)
    }
}
