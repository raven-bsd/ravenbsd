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
use crate::table::{ConnectionKey, ConnectionTable, Rebind, TimeWaitInfo, rebind_decision};

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
    detached: Vec<(ConnectionKey, TimeWaitInfo, i64)>,
    /// MPTCP subflow handles over plain-TCP sockets (MPTCP1b).
    pub(crate) subflows: crate::mptcp_hooks::SubflowTable,
    next_id: u32,
    max: usize,
}

pub(crate) fn connection_key(
    connection: &crate::tcp_state::TcpConnection,
) -> Option<ConnectionKey> {
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
            detached: Vec::new(),
            subflows: crate::mptcp_hooks::SubflowTable::new(),
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

    /// Reclaim closed records whose application handle is gone, and move released TIME-WAIT
    /// connections out of the table so they stop counting against its size.
    pub(crate) fn reclaim(&mut self) -> usize {
        let dead: Vec<u32> = self
            .records
            .iter()
            .filter(|(_, record)| record.closed && record.app_closed)
            .map(|(id, _)| *id)
            .collect();
        let waiting: Vec<u32> = self
            .records
            .iter()
            .filter(|(_, record)| {
                record.app_closed
                    && !record.closed
                    && matches!(&record.kind, Kind::Tcp(connection)
                        if connection.state == crate::tcp_state::TcpState::TimeWait
                            && connection.remote.is_some()
                            && connection.time_wait_deadline.is_some())
            })
            .map(|(id, _)| *id)
            .collect();
        for id in &waiting {
            self.unindex(SocketId(*id));
            let Some(record) = self.records.remove(id) else {
                continue;
            };
            let window = crate::tcp_data::advertised_window(&record);
            if let Kind::Tcp(connection) = &record.kind
                && let Some(key) = connection_key(connection)
                && let Some(deadline) = connection.time_wait_deadline
            {
                self.detached.push((
                    key,
                    TimeWaitInfo {
                        snd_nxt: connection.snd_nxt,
                        rcv_nxt: connection.rcv_nxt,
                        irs: connection.irs,
                        window,
                        ts_recent: connection.ts_recent,
                        timestamps: connection.timestamps,
                    },
                    deadline.as_millis(),
                ));
            }
        }
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
        dead.len() + waiting.len()
    }

    /// TIME-WAIT connections whose records were reclaimed since the last call.
    pub(crate) fn take_detached(&mut self) -> Vec<(ConnectionKey, TimeWaitInfo, i64)> {
        core::mem::take(&mut self.detached)
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
        self.tcp_listen_reuse(port, backlog, false)
    }

    /// Listen with the caller's `SO_REUSEADDR` policy.
    ///
    /// FreeBSD `sys/netinet/in_pcb.c:in_pcbbind_setup` lets a bind reuse a local port whose only
    /// holders are `TIME_WAIT` sockets when `SO_REUSEADDR` is set; NetBSD `in_pcb.c:in_pcbbind`
    /// and Linux `net/ipv4/inet_connection_sock.c:inet_csk_get_port` carry the same rule. Raven
    /// already documents that decision in [`rebind_decision`]; this is the missing wiring.
    pub fn tcp_listen_reuse(
        &mut self,
        port: u16,
        backlog: usize,
        reuse_addr: bool,
    ) -> Result<SocketId, Errno> {
        if port == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        let (existing_listener, existing_tcp, time_wait) = self.port_holders(port);
        if rebind_decision(
            false,
            existing_listener,
            existing_tcp,
            time_wait,
            reuse_addr,
        ) == Rebind::AddrInUse
        {
            return Err(Errno::EADDRINUSE);
        }
        let rx_cap = self.recv_buffer_cap();
        let tx_cap = self.send_buffer_cap();
        // FreeBSD `sys/kern/uipc_socket.c:solisten_proto` clamps the backlog to `somaxconn`;
        // `solisten_clone` refuses a child only when `sol_qlen > 3 * sol_qlimit / 2`.  The
        // completed queue therefore holds up to `3 * backlog / 2 + 1` children before the
        // overflow branch rejects the next clone.  ACCEPT_QUEUE_CEILING is the `somaxconn`
        // analogue; the partial queue keeps the same backlog bound at SYN time.
        let backlog = backlog.clamp(1, ACCEPT_QUEUE_CEILING);
        let completed_capacity = backlog
            .saturating_mul(3)
            .saturating_div(2)
            .saturating_add(1)
            .min(ACCEPT_QUEUE_CEILING);
        self.sockets.alloc(
            Kind::Listener {
                port,
                _backlog: backlog,
                pending: AcceptQueue::new(completed_capacity),
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
                let Kind::Listener {
                    pending, reported, ..
                } = &mut record.kind
                else {
                    return Err(Errno::ENOTSOCK);
                };
                let popped = pending.pop().map(|entry| SocketId(entry.socket));
                if popped.is_some() {
                    // `reported` counts the announced prefix; removing the front entry must
                    // shorten it, or the entry that moves into its place counts as announced
                    // and a connection completing around an accept never wakes the poller.
                    *reported = reported.saturating_sub(1);
                }
                popped
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
    /// FreeBSD `sys/netinet/tcp_usrreq.c:tcp_usr_connect` rejects a multicast destination with
    /// `EAFNOSUPPORT` and the limited broadcast address with `EACCES` before it enters the
    /// connection state machine.  The address-family and sockaddr-length checks are carried by
    /// the typed `IpAddr` API.
    pub fn tcp_connect<A: Into<IpAddr>>(
        &mut self,
        remote: (A, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        let remote = (remote.0.into(), remote.1);
        match remote.0 {
            IpAddr::V4(address) if address.is_multicast() => return Err(Errno::EAFNOSUPPORT),
            IpAddr::V4(address) if address.is_broadcast() => return Err(Errno::EACCES),
            IpAddr::V6(address) if address.is_multicast() => return Err(Errno::EAFNOSUPPORT),
            _ => {}
        }
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
    ///
    /// FreeBSD `sys/netinet/tcp_usrreq.c:tcp_usr_shutdown` returns `ENOTCONN` for a listener or
    /// a socket that is not connected, marks the send side with `socantsendmore`, and runs
    /// `tcp_usrclosed`.  An embryonic connection is not moved to a FIN state immediately:
    /// `tcp_usrclosed` sets `TF_NEEDFIN` for `SYN_SENT`/`SYN_RECEIVED`, and the FIN is emitted
    /// once the handshake completes.
    pub fn shutdown_write(&mut self, socket: SocketId) -> Result<(), Errno> {
        let state = {
            let record = self.sockets.record(socket)?;
            if record.closed {
                return Err(Errno::EBADF);
            }
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTCONN);
            };
            connection.state
        };
        if matches!(
            state,
            crate::tcp_state::TcpState::Closed | crate::tcp_state::TcpState::Listen
        ) {
            return Err(Errno::ENOTCONN);
        }
        if let Ok(record) = self.sockets.record_mut(socket) {
            record.tx_shutdown = true;
        }
        match state {
            crate::tcp_state::TcpState::SynSent | crate::tcp_state::TcpState::SynReceived => {
                let record = self.sockets.record_mut(socket)?;
                if let Kind::Tcp(connection) = &mut record.kind {
                    // TF_NEEDFIN: tcp_output emits the FIN after the connection is established.
                    connection.close_requested = true;
                }
                Ok(())
            }
            _ => self.tcp_shutdown(socket),
        }
    }

    /// Read and clear the pending socket error.
    ///
    /// FreeBSD `sys/kern/uipc_socket.c:sogetopt` returns `so_error` and clears it; the caller
    /// reads it once and the next read is 0. Raven keeps one `record.error` for both directions,
    /// the recorded divergence from the separate `so_error`/`so_rerror` pair.
    pub fn take_error(&mut self, socket: SocketId) -> Result<Option<Errno>, Errno> {
        Ok(self.sockets.record_mut(socket)?.error.take())
    }

    /// Close a socket and release its buffers.
    ///
    /// TCP close follows FreeBSD `sys/netinet/tcp_usrreq.c:tcp_usr_close` and `tcp_disconnect`:
    /// a socket below `TCPS_ESTABLISHED` is dropped by `tcp_close` without a FIN, a
    /// `TIME_WAIT` socket is left alone, and an established connection flushes its receive
    /// buffer and runs `tcp_usrclosed` (`FIN_WAIT_1` from `ESTABLISHED`, `LAST_ACK` from
    /// `CLOSE_WAIT`).  Queued send data is kept and drains before the FIN, as `tcp_disconnect`
    /// never flushes `so_snd`.
    pub fn close(&mut self, socket: SocketId) -> Result<(), Errno> {
        if let Ok(record) = self.sockets.record(socket)
            && matches!(record.kind, Kind::Tcp(_))
        {
            return self.tcp_disconnect(socket);
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
            half_open.clear();
        }
        if let Kind::Udp { datagrams, .. } = &mut record.kind {
            datagrams.clear();
        }
        // FreeBSD `sys/kern/uipc_socket.c:soclose` swaps both the incomplete and complete
        // queues out of a listening socket and `soabort`s every child.  A synchronized child
        // sends its RST through `tcp_usr_abort` -> `tcp_drop`, and the record becomes
        // reclaimable instead of remaining an unusable orphan.
        for raw in orphans {
            let _ = self.abort(SocketId(raw));
            if let Some(child) = self.sockets.records.get_mut(&raw) {
                child.app_closed = true;
            }
        }
        Ok(())
    }

    /// FreeBSD `tcp_disconnect` for a TCP socket.
    fn tcp_disconnect(&mut self, socket: SocketId) -> Result<(), Errno> {
        let (state, app_closed) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            (connection.state, record.app_closed)
        };
        if app_closed {
            return Err(Errno::EBADF);
        }
        if state == crate::tcp_state::TcpState::TimeWait {
            // `tcp_usr_close` skips `tcp_disconnect` in TIME_WAIT; the 2MSL timer owns it.
            let record = self.sockets.record_mut(socket)?;
            record.tx_shutdown = true;
            record.app_closed = true;
            return Ok(());
        }
        if matches!(
            state,
            crate::tcp_state::TcpState::Closed
                | crate::tcp_state::TcpState::Listen
                | crate::tcp_state::TcpState::SynSent
                | crate::tcp_state::TcpState::SynReceived
        ) {
            // `tcp_disconnect` calls `tcp_close` below TCPS_ESTABLISHED, so no FIN is emitted.
            self.release_embryonic(socket)?;
            return Ok(());
        }
        match self.tcp_shutdown(socket) {
            Ok(()) => {}
            // The connection already ended (reset, timeout): releasing the handle is fine
            // once; only a second close is a bad handle.
            Err(Errno::EBADF) => {}
            Err(error) => return Err(error),
        }
        let record = self.sockets.record_mut(socket)?;
        record.tx_shutdown = true;
        record.app_closed = true;
        // `tcp_disconnect` flushes so_rcv before entering the shutdown states; so_snd stays.
        record.rx.clear();
        Ok(())
    }

    /// Drop an embryonic TCP record without emitting a FIN (`tcp_disconnect` -> `tcp_close`).
    fn release_embryonic(&mut self, socket: SocketId) -> Result<(), Errno> {
        let raw = socket.0;
        let port = {
            let record = self.sockets.record(socket)?;
            match &record.kind {
                Kind::Tcp(connection) => connection.local_port,
                _ => return Err(Errno::ENOTSOCK),
            }
        };
        let listener = self.sockets.records.iter().find_map(|(id, record)| {
            if record.closed {
                return None;
            }
            match record.kind {
                Kind::Listener { port: bound, .. } if bound == port => Some(*id),
                _ => None,
            }
        });
        if let Some(listener) = listener
            && let Ok(record) = self.sockets.record_mut(SocketId(listener))
            && let Kind::Listener { half_open, .. } = &mut record.kind
        {
            // `soabort` removes the child from the incomplete queue before it is freed.
            half_open.remove_socket(raw);
        }
        self.sockets.unindex(socket);
        let record = self.sockets.record_mut(socket)?;
        record.closed = true;
        record.app_closed = true;
        record.tx_shutdown = true;
        record.rx.clear();
        record.tx.clear();
        if let Kind::Tcp(connection) = &mut record.kind {
            connection.state = crate::tcp_state::TcpState::Closed;
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
        let result = (datagram.data.len(), datagram.src);
        self.resync_readable_baseline(socket);
        Ok(result)
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

    /// Classify the live holders of `port` for [`rebind_decision`].
    ///
    /// A `TIME_WAIT` record is reported separately because `SO_REUSEADDR` may override it; a
    /// listener, an established connection or a UDP binding is a hard conflict. UDP shares the
    /// TCP "existing" bucket to keep the pre-existing port-conflict policy for this revision.
    fn port_holders(&self, port: u16) -> (bool, bool, bool) {
        let mut listener = false;
        let mut existing = false;
        let mut time_wait = false;
        for record in self.sockets.records.values() {
            if record.closed {
                continue;
            }
            match &record.kind {
                Kind::Listener { port: local, .. } if *local == port => listener = true,
                Kind::Udp {
                    local_port: local, ..
                } if *local == port => existing = true,
                Kind::Tcp(connection) if connection.local_port == port => {
                    if connection.state == crate::tcp_state::TcpState::TimeWait {
                        time_wait = true;
                    } else {
                        existing = true;
                    }
                }
                _ => {}
            }
        }
        (listener, existing, time_wait)
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
