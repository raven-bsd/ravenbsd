// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded socket table and the non-blocking socket API (contract C15).

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::device::Device;
use crate::event::SocketId;
use crate::stack::{Ipv4Addr, Stack};

pub(crate) struct Datagram {
    pub(crate) data: Vec<u8>,
    pub(crate) src: (Ipv4Addr, u16),
}

pub(crate) enum Kind {
    Listener {
        port: u16,
        _backlog: usize,
        pending: VecDeque<SocketId>,
        reported: usize,
    },
    Tcp {
        _remote: Option<(Ipv4Addr, u16)>,
        local_port: u16,
    },
    Udp {
        local_port: u16,
        datagrams: VecDeque<Datagram>,
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
    ///
    /// `local_port` defaults to an ephemeral port. This skeleton allocates the socket record;
    /// the handshake is implemented by C08.
    pub fn tcp_connect(
        &mut self,
        remote: (Ipv4Addr, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        if remote.1 == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        let local_port = match local_port {
            Some(0) => return Err(Errno::EADDRNOTAVAIL),
            Some(port) if self.port_in_use(port) => return Err(Errno::EADDRINUSE),
            Some(port) => port,
            None => self.alloc_ephemeral_port()?,
        };
        let rx_cap = self.config.rx_queue_bytes;
        let tx_cap = self.config.tx_queue_bytes;
        self.sockets.alloc(
            Kind::Tcp {
                _remote: Some(remote),
                local_port,
            },
            rx_cap,
            tx_cap,
        )
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
            } else if !matches!(record.kind, Kind::Tcp { .. }) {
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
        if !matches!(record.kind, Kind::Tcp { .. }) {
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
        let record = self.sockets.record_mut(socket)?;
        if record.closed {
            return Err(Errno::EBADF);
        }
        if !matches!(record.kind, Kind::Tcp { .. }) {
            return Err(Errno::ENOTCONN);
        }
        record.tx_shutdown = true;
        Ok(())
    }

    /// Close a socket and release its buffers.
    pub fn close(&mut self, socket: SocketId) -> Result<(), Errno> {
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

    /// Bind a UDP socket on `port`.
    pub fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno> {
        if port == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        if self.port_in_use(port) {
            return Err(Errno::EADDRINUSE);
        }
        let rx_cap = self.config.rx_queue_bytes;
        let tx_cap = self.config.tx_queue_bytes;
        self.sockets.alloc(
            Kind::Udp {
                local_port: port,
                datagrams: VecDeque::new(),
            },
            rx_cap,
            tx_cap,
        )
    }

    /// Send a UDP datagram to `to`. The skeleton validates and bounds the write; C06 adds the
    /// wire path.
    pub fn udp_send_to(
        &mut self,
        socket: SocketId,
        data: &[u8],
        to: (Ipv4Addr, u16),
    ) -> Result<usize, Errno> {
        if to.1 == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        let mut dropped = 0usize;
        let outcome = {
            let record = self.sockets.record_mut(socket)?;
            if record.closed {
                Err(Errno::EBADF)
            } else if !matches!(record.kind, Kind::Udp { .. }) {
                Err(Errno::ENOTSOCK)
            } else if data.len() > record.tx_cap {
                dropped = data.len();
                Err(Errno::EAGAIN)
            } else {
                Ok(data.len())
            }
        };
        self.drops = self.drops.saturating_add(dropped);
        outcome
    }

    /// Receive a UDP datagram and its source address.
    pub fn udp_recv_from(
        &mut self,
        socket: SocketId,
        buffer: &mut [u8],
    ) -> Result<(usize, (Ipv4Addr, u16)), Errno> {
        let record = self.sockets.record_mut(socket)?;
        let Kind::Udp { datagrams, .. } = &mut record.kind else {
            return Err(Errno::ENOTSOCK);
        };
        let Some(datagram) = datagrams.pop_front() else {
            return Err(Errno::EAGAIN);
        };
        if datagram.data.len() > buffer.len() {
            return Err(Errno::EINVAL);
        }
        buffer[..datagram.data.len()].copy_from_slice(&datagram.data);
        Ok((datagram.data.len(), datagram.src))
    }

    /// Read and discard a UDP datagram, returning its payload length.
    pub fn udp_recv(&mut self, socket: SocketId, buffer: &mut [u8]) -> Result<usize, Errno> {
        self.udp_recv_from(socket, buffer).map(|(length, _)| length)
    }

    fn port_in_use(&self, port: u16) -> bool {
        self.sockets.records.values().any(|record| {
            if record.closed {
                return false;
            }
            match &record.kind {
                Kind::Listener { port: local, .. }
                | Kind::Tcp {
                    local_port: local, ..
                }
                | Kind::Udp {
                    local_port: local, ..
                } => *local == port,
            }
        })
    }

    fn alloc_ephemeral_port(&mut self) -> Result<u16, Errno> {
        const START: u32 = 49_152;
        const COUNT: u32 = u32::MAX - START + 1;
        for offset in 0..COUNT {
            let port =
                u16::try_from((START + offset) % 65_536).map_err(|_| Errno::EADDRNOTAVAIL)?;
            if port >= 49_152 && !self.port_in_use(port) {
                return Ok(port);
            }
        }
        Err(Errno::EADDRNOTAVAIL)
    }
}
