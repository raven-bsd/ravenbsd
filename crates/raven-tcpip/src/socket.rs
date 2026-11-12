// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded socket table and the non-blocking socket API (contract C15).

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::udp::UdpBuilder;

use crate::device::Device;
use crate::event::SocketId;
use crate::ipv4_stack::to_wire;
use crate::ipv6_stack::IpAddr;
use crate::stack::Stack;

pub(crate) struct Datagram {
    pub(crate) data: Vec<u8>,
    pub(crate) src: (IpAddr, u16),
}

pub(crate) enum Kind {
    Listener {
        port: u16,
        _backlog: usize,
        pending: VecDeque<SocketId>,
        half_open: VecDeque<SocketId>,
        reported: usize,
    },
    Tcp(crate::tcp_state::TcpConnection),
    Udp {
        local_port: u16,
        datagrams: VecDeque<Datagram>,
        remote: Option<(IpAddr, u16)>,
    },
    Icmp {
        id: u16,
    },
}

pub(crate) struct SocketRecord {
    pub(crate) kind: Kind,
    pub(crate) rx: VecDeque<u8>,
    pub(crate) tx: VecDeque<u8>,
    #[allow(dead_code)] // Read by the test hooks and the transport layers added by C06-C09.
    pub(crate) rx_cap: usize,
    pub(crate) tx_cap: usize,
    pub(crate) tx_shutdown: bool,
    pub(crate) closed: bool,
    pub(crate) peer_closed: bool,
    pub(crate) error: Option<Errno>,
    pub(crate) prev_readable: bool,
    pub(crate) prev_writable: bool,
    pub(crate) prev_closed: bool,
    pub(crate) prev_error: Option<Errno>,
    pub(crate) forced_readable: bool,
}

impl SocketRecord {
    fn new(kind: Kind, rx_cap: usize, tx_cap: usize) -> Self {
        Self {
            kind,
            rx: VecDeque::new(),
            tx: VecDeque::new(),
            rx_cap,
            tx_cap,
            tx_shutdown: false,
            closed: false,
            peer_closed: false,
            error: None,
            prev_readable: false,
            prev_writable: false,
            prev_closed: false,
            prev_error: None,
            forced_readable: false,
        }
    }
}

pub(crate) struct SocketTable {
    pub(crate) records: BTreeMap<u32, SocketRecord>,
    next_id: u32,
    max: usize,
}

impl SocketTable {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            records: BTreeMap::new(),
            next_id: 1,
            max,
        }
    }

    pub(crate) fn alloc(
        &mut self,
        kind: Kind,
        rx_cap: usize,
        tx_cap: usize,
    ) -> Result<SocketId, Errno> {
        if self.records.len() >= self.max {
            return Err(Errno::EMFILE);
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.records
            .insert(id, SocketRecord::new(kind, rx_cap, tx_cap));
        Ok(SocketId(id))
    }

    pub(crate) fn record(&self, id: SocketId) -> Result<&SocketRecord, Errno> {
        self.records.get(&id.0).ok_or(Errno::EBADF)
    }

    pub(crate) fn record_mut(&mut self, id: SocketId) -> Result<&mut SocketRecord, Errno> {
        self.records.get_mut(&id.0).ok_or(Errno::EBADF)
    }
}

impl<D: Device> Stack<D> {
    /// Listen for TCP connections on `port`.
    ///
    /// `backlog` is stored for the connection layer; this skeleton does not complete handshakes.
    /// Port zero is rejected with [`Errno::EADDRNOTAVAIL`], a duplicate with
    /// [`Errno::EADDRINUSE`].
    pub fn tcp_listen(&mut self, port: u16, backlog: usize) -> Result<SocketId, Errno> {
        if port == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        if self.port_in_use(port) {
            return Err(Errno::EADDRINUSE);
        }
        let rx_cap = self.config.rx_queue_bytes;
        let tx_cap = self.config.tx_queue_bytes;
        self.sockets.alloc(
            Kind::Listener {
                port,
                _backlog: backlog.max(1),
                pending: VecDeque::new(),
                half_open: VecDeque::new(),
                reported: 0,
            },
            rx_cap,
            tx_cap,
        )
    }

    /// Take one completed connection from a listener.
    pub fn tcp_accept(&mut self, listener: SocketId) -> Result<Option<SocketId>, Errno> {
        {
            let record = self.sockets.record(listener)?;
            if record.closed {
                return Err(Errno::EBADF);
            }
            if !matches!(record.kind, Kind::Listener { .. }) {
                return Err(Errno::ENOTSOCK);
            }
        }
        loop {
            let next = {
                let record = self.sockets.record_mut(listener)?;
                let Kind::Listener { pending, .. } = &mut record.kind else {
                    return Err(Errno::ENOTSOCK);
                };
                pending.pop_front()
            };
            let Some(connection) = next else {
                return Ok(None);
            };
            let usable = match self.sockets.records.get(&connection.0) {
                Some(record) => !record.closed,
                None => false,
            };
            if usable {
                return Ok(Some(connection));
            }
        }
    }

    /// Start a TCP connection to `remote`.
    pub fn tcp_connect<A: Into<IpAddr>>(
        &mut self,
        remote: (A, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        self.tcp_connect_internal(remote, local_port)
    }

    /// Queue bytes on a TCP connection. Returns [`Errno::EAGAIN`] when the transmit buffer is
    /// full; the dropped bytes are counted in the next [`crate::PollResult`].
    pub fn send(&mut self, socket: SocketId, data: &[u8]) -> Result<usize, Errno> {
        if data.is_empty() {
            return Ok(0);
        }
        let mut dropped = 0usize;
        let outcome = {
            let record = self.sockets.record_mut(socket)?;
            if record.closed {
                Err(Errno::EBADF)
            } else if !matches!(record.kind, Kind::Tcp(_)) {
                Err(Errno::ENOTCONN)
            } else if record.tx_shutdown {
                Err(Errno::EPIPE)
            } else {
                let room = record.tx_cap.saturating_sub(record.tx.len());
                if data.len() > room {
                    dropped = data.len();
                    Err(Errno::EAGAIN)
                } else {
                    record.tx.extend(data.iter().copied());
                    Ok(data.len())
                }
            }
        };
        self.drops = self.drops.saturating_add(dropped);
        outcome
    }

    /// Read bytes from a TCP connection.
    ///
    /// An empty non-closed socket returns [`Errno::EAGAIN`]; an orderly EOF returns `Ok(0)`.
    pub fn recv(&mut self, socket: SocketId, buffer: &mut [u8]) -> Result<usize, Errno> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let record = self.sockets.record_mut(socket)?;
        if record.closed && record.rx.is_empty() {
            return Ok(0);
        }
        if !matches!(record.kind, Kind::Tcp(_) | Kind::Icmp { .. }) {
            return Err(Errno::ENOTSOCK);
        }
        let count = buffer.len().min(record.rx.len());
        for (destination, source) in buffer.iter_mut().zip(record.rx.drain(..count)) {
            *destination = source;
        }
        if count == 0 {
            if record.peer_closed {
                Ok(0)
            } else {
                Err(Errno::EAGAIN)
            }
        } else {
            Ok(count)
        }
    }

    /// Close the transmit half of a TCP connection.
    pub fn shutdown_write(&mut self, socket: SocketId) -> Result<(), Errno> {
        {
            let record = self.sockets.record(socket)?;
            if record.closed {
                return Err(Errno::EBADF);
            }
            if !matches!(record.kind, Kind::Tcp(_)) {
                return Err(Errno::ENOTCONN);
            }
        }
        if let Ok(record) = self.sockets.record_mut(socket) {
            record.tx_shutdown = true;
        }
        self.tcp_shutdown(socket)
    }

    /// Close a socket and release its buffers.
    pub fn close(&mut self, socket: SocketId) -> Result<(), Errno> {
        if let Ok(record) = self.sockets.record(socket)
            && matches!(record.kind, Kind::Tcp(_))
        {
            self.tcp_shutdown(socket)?;
            let record = self.sockets.record_mut(socket)?;
            record.tx_shutdown = true;
            record.tx.clear();
            return Ok(());
        }
        let record = self.sockets.record_mut(socket)?;
        if record.closed {
            return Err(Errno::EBADF);
        }
        record.closed = true;
        record.rx.clear();
        record.tx.clear();
        if let Kind::Listener { pending, .. } = &mut record.kind {
            pending.clear();
        }
        if let Kind::Udp { datagrams, .. } = &mut record.kind {
            datagrams.clear();
        }
        Ok(())
    }

    /// Bind a UDP socket on `port`; zero allocates a deterministic ephemeral port.
    pub fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno> {
        let port = if port == 0 {
            self.alloc_ephemeral_port()?
        } else {
            if self.port_in_use(port) {
                return Err(Errno::EADDRINUSE);
            }
            port
        };
        let rx_cap = self.config.rx_queue_bytes;
        let tx_cap = self.config.tx_queue_bytes;
        self.sockets.alloc(
            Kind::Udp {
                local_port: port,
                datagrams: VecDeque::new(),
                remote: None,
            },
            rx_cap,
            tx_cap,
        )
    }

    /// Send a UDP datagram to `to`, computing the checksum and handing it to IPv4.
    pub fn udp_send_to<A: Into<IpAddr>>(
        &mut self,
        socket: SocketId,
        data: &[u8],
        to: (A, u16),
    ) -> Result<usize, Errno> {
        let to = (to.0.into(), to.1);
        if to.1 == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        let mut dropped = 0usize;
        let (local_port, pending_error) = {
            let record = self.sockets.record_mut(socket)?;
            if record.closed {
                return Err(Errno::EBADF);
            }
            let local_port = match &record.kind {
                Kind::Udp { local_port, .. } => *local_port,
                _ => return Err(Errno::ENOTSOCK),
            };
            if data.len() > record.tx_cap {
                dropped = data.len();
                (local_port, Some(Errno::EAGAIN))
            } else {
                (local_port, record.error.take())
            }
        };
        if let Some(error) = pending_error {
            self.drops = self.drops.saturating_add(dropped);
            return Err(error);
        }
        match to.0 {
            IpAddr::V4(destination) => {
                let local = self.ipv4.ok_or(Errno::ENETUNREACH)?.addr;
                let mut datagram = vec![0u8; 8 + data.len()];
                {
                    let context = ChecksumContext::ipv4(to_wire(local), to_wire(destination));
                    let mut builder =
                        UdpBuilder::new(&mut datagram, context, local_port, to.1, data.len())
                            .map_err(|_| Errno::EINVAL)?;
                    builder.payload_mut().copy_from_slice(data);
                    builder.finish();
                }
                self.send_ipv4(&datagram, destination, 17)?;
            }
            IpAddr::V6(destination) => {
                let local = self.usable_ipv6().ok_or(Errno::ENETUNREACH)?;
                let mut datagram = vec![0u8; 8 + data.len()];
                {
                    let context = ChecksumContext::ipv6(
                        crate::ipv6_stack::to_wire(local),
                        crate::ipv6_stack::to_wire(destination),
                    );
                    let mut builder =
                        UdpBuilder::new(&mut datagram, context, local_port, to.1, data.len())
                            .map_err(|_| Errno::EINVAL)?;
                    builder.payload_mut().copy_from_slice(data);
                    builder.finish();
                }
                self.send_ipv6(&datagram, destination, 17)?;
            }
        }
        if let Kind::Udp { remote, .. } = &mut self.sockets.record_mut(socket)?.kind {
            *remote = Some(to);
        }
        Ok(data.len())
    }

    /// Receive a UDP datagram and its source address.
    ///
    /// A short buffer receives the prefix and the return value is the true datagram length, so
    /// the caller can detect truncation.
    pub fn udp_recv_from(
        &mut self,
        socket: SocketId,
        buffer: &mut [u8],
    ) -> Result<(usize, (IpAddr, u16)), Errno> {
        let record = self.sockets.record_mut(socket)?;
        if record.closed {
            return Err(Errno::EBADF);
        }
        if let Some(error) = record.error.take() {
            return Err(error);
        }
        let Kind::Udp { datagrams, .. } = &mut record.kind else {
            return Err(Errno::ENOTSOCK);
        };
        let Some(datagram) = datagrams.pop_front() else {
            return Err(Errno::EAGAIN);
        };
        let count = buffer.len().min(datagram.data.len());
        buffer[..count].copy_from_slice(&datagram.data[..count]);
        Ok((datagram.data.len(), datagram.src))
    }

    /// Read and discard a UDP datagram, returning its payload length.
    pub fn udp_recv(&mut self, socket: SocketId, buffer: &mut [u8]) -> Result<usize, Errno> {
        self.udp_recv_from(socket, buffer).map(|(length, _)| length)
    }

    /// The bound local port of a UDP socket (host tests).
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_udp_port(&self, socket: SocketId) -> Result<u16, Errno> {
        match &self.sockets.record(socket)?.kind {
            Kind::Udp { local_port, .. } => Ok(*local_port),
            _ => Err(Errno::ENOTSOCK),
        }
    }

    pub(crate) fn port_in_use(&self, port: u16) -> bool {
        self.sockets.records.values().any(|record| {
            if record.closed {
                return false;
            }
            match &record.kind {
                Kind::Listener { port: local, .. }
                | Kind::Udp {
                    local_port: local, ..
                } => *local == port,
                Kind::Tcp(connection) => connection.local_port == port,
                Kind::Icmp { .. } => false,
            }
        })
    }

    pub(crate) fn alloc_ephemeral_port(&mut self) -> Result<u16, Errno> {
        const START: u32 = 49_152;
        const COUNT: u32 = 65_536 - START;
        let first = u32::from(self.next_ephemeral);
        for offset in 0..COUNT {
            let port = u16::try_from(START + (first - START + offset) % COUNT)
                .map_err(|_| Errno::EADDRNOTAVAIL)?;
            if !self.port_in_use(port) {
                self.next_ephemeral = if port == 65_535 { 49_152 } else { port + 1 };
                return Ok(port);
            }
        }
        Err(Errno::EADDRNOTAVAIL)
    }
}
