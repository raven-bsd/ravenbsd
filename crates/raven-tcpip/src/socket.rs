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
use crate::listen::{AcceptQueue, HalfOpenQueue};
use crate::stack::Stack;
use crate::table::{ConnectionKey, ConnectionTable};

/// Hard ceiling of a listener's accept queue; the runtime limit is the
/// `tcp.accept_queue_limit` property.
const ACCEPT_QUEUE_CEILING: usize = 65_535;

pub(crate) struct Datagram {
    pub(crate) data: Vec<u8>,
    pub(crate) src: (IpAddr, u16),
}

#[allow(clippy::large_enum_variant)] // One connection record is intentionally inline for stable ids.
pub(crate) enum Kind {
    Listener {
        port: u16,
        _backlog: usize,
        pending: AcceptQueue,
        half_open: HalfOpenQueue,
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
    /// The application released its handle; the record may be reclaimed once it is closed.
    pub(crate) app_closed: bool,
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
            app_closed: false,
        }
    }
}

pub(crate) struct SocketTable {
    pub(crate) records: BTreeMap<u32, SocketRecord>,
    /// Live four-tuple classification of TCP connections (C17).
    conn_index: ConnectionTable<u32>,
    reported_chains: usize,
    released_ports: Vec<u16>,
    next_id: u32,
    max: usize,
}

pub(crate) fn connection_key(connection: &crate::tcp_state::TcpConnection) -> Option<ConnectionKey> {
    let (remote, remote_port) = connection.remote?;
    Some(ConnectionKey {
        local: connection.local,
        local_port: connection.local_port,
        remote,
        remote_port,
    })
}

impl SocketTable {
    pub(crate) fn new(max: usize, secret: u64, max_chain: usize) -> Self {
        Self {
            records: BTreeMap::new(),
            conn_index: ConnectionTable::new(max.max(1), secret, max_chain),
            reported_chains: 0,
            released_ports: Vec::new(),
            next_id: 1,
            max,
        }
    }

    /// Classify a segment's four-tuple to the open connection that owns it.
    pub(crate) fn find_connection(
        &self,
        local: IpAddr,
        local_port: u16,
        remote: IpAddr,
        remote_port: u16,
    ) -> Option<SocketId> {
        let key = ConnectionKey {
            local,
            local_port,
            remote,
            remote_port,
        };
        let id = *self.conn_index.get(&key)?;
        let record = self.records.get(&id)?;
        match &record.kind {
            Kind::Tcp(connection) if !record.closed && connection_key(connection) == Some(key) => {
                Some(SocketId(id))
            }
            _ => None,
        }
    }

    /// Chains longer than the configured bound seen by the live index, since the last call.
    pub(crate) fn take_long_chains(&mut self) -> usize {
        let total = self.conn_index.stats.long_chain_warnings;
        let fresh = total.saturating_sub(self.reported_chains);
        self.reported_chains = total;
        fresh
    }

    /// Change the collision-chain bound the live index reports against.
    pub(crate) fn set_index_max_chain(&mut self, max_chain: usize) {
        self.conn_index.set_max_chain(max_chain);
    }

    /// Drop the index entry of a connection that closed.
    pub(crate) fn unindex(&mut self, id: SocketId) {
        let key = match self.records.get(&id.0).map(|record| &record.kind) {
            Some(Kind::Tcp(connection)) => connection_key(connection),
            _ => None,
        };
        if let Some(key) = key
            && self.conn_index.get(&key) == Some(&id.0)
        {
            self.conn_index.remove(&key);
        }
    }

    /// Reclaim closed records whose application handle is gone.
    pub(crate) fn reclaim(&mut self) -> usize {
        let dead: Vec<u32> = self
            .records
            .iter()
            .filter(|(_, record)| record.closed && record.app_closed)
            .map(|(id, _)| *id)
            .collect();
        for id in &dead {
            self.unindex(SocketId(*id));
            if let Some(record) = self.records.remove(id) {
                match record.kind {
                    Kind::Tcp(connection) => self.released_ports.push(connection.local_port),
                    Kind::Udp { local_port, .. } => self.released_ports.push(local_port),
                    Kind::Listener { port, .. } => self.released_ports.push(port),
                    Kind::Icmp { .. } => {}
                }
            }
        }
        dead.len()
    }

    /// Local ports of the records reclaimed since the last call.
    pub(crate) fn take_released_ports(&mut self) -> Vec<u16> {
        core::mem::take(&mut self.released_ports)
    }

    pub(crate) fn alloc(
        &mut self,
        kind: Kind,
        rx_cap: usize,
        tx_cap: usize,
    ) -> Result<SocketId, Errno> {
        if self.records.len() >= self.max {
            self.reclaim();
        }
        if self.records.len() >= self.max {
            return Err(Errno::EMFILE);
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if let Kind::Tcp(connection) = &kind
            && let Some(key) = connection_key(connection)
        {
            self.conn_index.insert(key, id);
        }
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
        let rx_cap = self.recv_buffer_cap();
        let tx_cap = self.send_buffer_cap();
        let backlog = backlog.max(1);
        self.sockets.alloc(
            Kind::Listener {
                port,
                _backlog: backlog,
                pending: AcceptQueue::new(ACCEPT_QUEUE_CEILING),
                half_open: HalfOpenQueue::new(backlog),
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
                pending.pop().map(|entry| SocketId(entry.socket))
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

    /// Queue bytes on a TCP connection. Returns the accepted count, or `EAGAIN` when full.
    pub fn send(&mut self, socket: SocketId, data: &[u8]) -> Result<usize, Errno> {
        self.tcp_send_data(socket, data)
    }

    /// Read bytes from a TCP connection, or a datagram from a UDP socket.
    pub fn recv(&mut self, socket: SocketId, buffer: &mut [u8]) -> Result<usize, Errno> {
        if matches!(self.sockets.record(socket)?.kind, Kind::Udp { .. }) {
            return self.udp_recv_from(socket, buffer).map(|(length, _)| length);
        }
        self.tcp_recv_data(socket, buffer)
    }

    /// Disable Nagle for a TCP socket.
    pub fn set_nodelay(&mut self, socket: SocketId, on: bool) -> Result<(), Errno> {
        self.tcp_set_nodelay(socket, on)
    }

    /// Bytes waiting to be sent.
    pub fn send_queue_len(&self, socket: SocketId) -> Result<usize, Errno> {
        self.tcp_send_queue_len(socket)
    }

    /// Bytes waiting to be received.
    pub fn recv_queue_len(&self, socket: SocketId) -> Result<usize, Errno> {
        self.tcp_recv_queue_len(socket)
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
            record.app_closed = true;
            record.tx.clear();
            return Ok(());
        }
        let record = self.sockets.record_mut(socket)?;
        if record.closed {
            return Err(Errno::EBADF);
        }
        record.closed = true;
        record.app_closed = true;
        record.rx.clear();
        record.tx.clear();
        let mut orphans: Vec<u32> = Vec::new();
        if let Kind::Listener {
            pending, half_open, ..
        } = &mut record.kind
        {
            orphans.extend(pending.sockets());
            orphans.extend(half_open.sockets());
            pending.clear();
        }
        if let Kind::Udp { datagrams, .. } = &mut record.kind {
            datagrams.clear();
        }
        for raw in orphans {
            if let Some(child) = self.sockets.records.get_mut(&raw) {
                child.app_closed = true;
            }
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
        let rx_cap = self.recv_buffer_cap();
        let tx_cap = self.send_buffer_cap();
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

    /// The send and receive byte caps of a socket (host tests).
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_socket_caps(&self, socket: SocketId) -> Result<(usize, usize), Errno> {
        let record = self.sockets.record(socket)?;
        Ok((record.rx_cap, record.tx_cap))
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
        for port in self.sockets.take_released_ports() {
            self.ports.release(port);
        }
        let attempts = usize::try_from(self.tunables.port_alloc_attempts)
            .unwrap_or(64)
            .max(1);
        let mut reclaimed = false;
        for _ in 0..attempts {
            let Some(port) = self.ports.allocate() else {
                if !reclaimed && self.sockets.reclaim() > 0 {
                    reclaimed = true;
                    for port in self.sockets.take_released_ports() {
                        self.ports.release(port);
                    }
                    continue;
                }
                break;
            };
            self.counters.port_alloc_probes = self
                .counters
                .port_alloc_probes
                .wrapping_add(u64::try_from(self.ports.last_attempts).unwrap_or(1));
            if !self.port_in_use(port) {
                let last = u16::try_from(self.tunables.port_range_last).unwrap_or(60_999);
                let first = u16::try_from(self.tunables.port_range_first).unwrap_or(32_768);
                self.next_ephemeral = if port >= last { first } else { port + 1 };
                return Ok(port);
            }
        }
        self.counters.port_alloc_fails = self.counters.port_alloc_fails.wrapping_add(1);
        Err(Errno::EADDRNOTAVAIL)
    }
}
