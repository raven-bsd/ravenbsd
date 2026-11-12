// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The thin, backend-generic socket core.

use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::backend::StackBackend;
use crate::{Event, Ipv4Addr, SocketId};

/// A nonblocking socket core over any [`StackBackend`].
pub struct Core<B: StackBackend> {
    backend: B,
}

impl<B: StackBackend> Core<B> {
    /// Wrap an existing backend.
    #[must_use]
    pub const fn from_backend(backend: B) -> Self {
        Self { backend }
    }

    /// The backend behind this core.
    #[must_use]
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable access to the backend (backend-specific test hooks).
    #[must_use]
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Configure a static IPv4 address.
    pub fn set_ipv4(&mut self, addr: Ipv4Addr, prefix: u8, gateway: Option<Ipv4Addr>) {
        self.backend.set_ipv4(addr, prefix, gateway);
    }

    /// Enable DHCP.
    pub fn enable_dhcp(&mut self) {
        self.backend.enable_dhcp();
    }

    /// Enable DHCP, or return [`Errno::EOPNOTSUPP`] when the backend has no DHCP client.
    pub fn try_enable_dhcp(&mut self) -> Result<(), Errno> {
        self.backend.try_enable_dhcp()
    }

    /// Listen for TCP connections.
    pub fn tcp_listen(&mut self, port: u16, backlog: usize) -> Result<SocketId, Errno> {
        self.backend.tcp_listen(port, backlog)
    }

    /// Accept one completed connection.
    pub fn tcp_accept(&mut self, listener: SocketId) -> Result<Option<SocketId>, Errno> {
        self.backend.tcp_accept(listener)
    }

    /// Start a TCP connection.
    pub fn tcp_connect(
        &mut self,
        remote: (Ipv4Addr, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        self.backend.tcp_connect(remote, local_port)
    }

    /// Queue bytes on a TCP connection.
    pub fn send(&mut self, s: SocketId, data: &[u8]) -> Result<usize, Errno> {
        self.backend.send(s, data)
    }

    /// Read bytes from a TCP connection.
    pub fn recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno> {
        self.backend.recv(s, buf)
    }

    /// Shut down the write side.
    pub fn shutdown_write(&mut self, s: SocketId) -> Result<(), Errno> {
        self.backend.shutdown_write(s)
    }

    /// Close a socket.
    pub fn close(&mut self, s: SocketId) -> Result<(), Errno> {
        self.backend.close(s)
    }

    /// Bind a UDP socket.
    pub fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno> {
        self.backend.udp_bind(port)
    }

    /// Send a UDP datagram.
    pub fn udp_send_to(
        &mut self,
        s: SocketId,
        data: &[u8],
        to: (Ipv4Addr, u16),
    ) -> Result<usize, Errno> {
        self.backend.udp_send_to(s, data, to)
    }

    /// Receive a UDP datagram.
    pub fn udp_recv_from(
        &mut self,
        s: SocketId,
        buf: &mut [u8],
    ) -> Result<(usize, (Ipv4Addr, u16)), Errno> {
        self.backend.udp_recv_from(s, buf)
    }

    /// Drive the backend and return readiness transitions.
    pub fn poll(&mut self, now_ms: i64) -> Vec<Event> {
        self.backend.poll(now_ms)
    }

    /// Time until the next poll.
    pub fn next_poll_delay_ms(&mut self, now_ms: i64) -> Option<u64> {
        self.backend.next_poll_delay_ms(now_ms)
    }
}

impl<D: raven_tcpip::Device> Core<crate::backend::raven::RavenTcpipBackend<D>> {
    /// Create the native raven-tcpip-backed core with Ethernet address `mac`.
    #[must_use]
    pub fn new(device: D, mac: [u8; 6], now_ms: i64) -> Self {
        Self {
            backend: crate::backend::raven::RavenTcpipBackend::new(device, mac, now_ms),
        }
    }
}
