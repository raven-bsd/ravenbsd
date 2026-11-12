// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The backend seam: the socket core knows only this trait.

use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::{Event, Ipv4Addr, SocketId};

#[cfg(feature = "backend-raven-tcpip")]
pub mod raven;
pub mod smoltcp;

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

    /// Enable DHCP, or report that this backend has no DHCP client.
    ///
    /// The default forwards to [`Self::enable_dhcp`] and succeeds. A backend without a client
    /// overrides it to return [`Errno::EOPNOTSUPP`] so a caller can tell "no lease will ever
    /// arrive" from "no lease yet". A native client needs the backend to send and receive UDP
    /// from the unconfigured address (broadcast from 0.0.0.0:68) and to let the lease replace
    /// the interface address, which is a change to the device-facing half of this trait.
    fn try_enable_dhcp(&mut self) -> Result<(), Errno> {
        self.enable_dhcp();
        Ok(())
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
}
