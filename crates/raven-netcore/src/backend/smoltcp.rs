// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The smoltcp implementation of [`StackBackend`].

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use smoltcp::iface::{Config as IfaceConfig, Interface, SocketHandle, SocketSet};
use smoltcp::phy::Device;
use smoltcp::socket::{dhcpv4, tcp, udp};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, IpEndpoint, Ipv4Cidr};

use crate::Ipv4Addr;
use crate::backend::StackBackend;

const TCP_BUFFER_SIZE: usize = 65_536;
const UDP_METADATA_SLOTS: usize = 32;
const UDP_PAYLOAD_SIZE: usize = 65_535;
const EPHEMERAL_START: u16 = 49_152;
const EPHEMERAL_COUNT: u32 = (u16::MAX - EPHEMERAL_START + 1) as u32;
const CONNECT_TIMEOUT_MS: i64 = 30_000;

use crate::{Event, SocketId};

/// A nonblocking socket core over a smoltcp [`Device`].
pub struct SmoltcpBackend<D: Device> {
    device: D,
    iface: Interface,
    sockets: SocketSet<'static>,
    records: BTreeMap<u32, Record>,
    next_id: u32,
    next_ephemeral: u16,
    now_ms: i64,
    dhcp: Option<SocketHandle>,
}

struct Record {
    handle: Option<SocketHandle>,
    kind: Kind,
    prev_readable: bool,
    prev_writable: bool,
    closed_reported: bool,
    closed: bool,
}

enum Kind {
    Listener(Listener),
    Tcp(TcpRecord),
    Udp { local_port: u16 },
}

struct Listener {
    port: u16,
    limit: usize,
    pool: Vec<SocketHandle>,
    pending: Vec<SocketId>,
}

struct TcpRecord {
    local_port: u16,
    last_state: tcp::State,
    started_ms: i64,
    ever_established: bool,
    locally_closed: bool,
}

impl<D: Device> SmoltcpBackend<D> {
    /// Create a core over `device` with Ethernet address `mac`.
    ///
    /// `now_ms` is the initial monotonic time in milliseconds. The device must use the Ethernet
    /// medium; the interface does no ARP on an IP-medium device.
    pub fn new(mut device: D, mac: [u8; 6], now_ms: i64) -> Self {
        let mut config = IfaceConfig::new(HardwareAddress::Ethernet(EthernetAddress(mac)));
        config.random_seed = random_seed(mac, now_ms);
        let iface = Interface::new(config, &mut device, Instant::from_millis(now_ms));
        Self {
            device,
            iface,
            sockets: SocketSet::new(Vec::new()),
            records: BTreeMap::new(),
            next_id: 1,
            next_ephemeral: EPHEMERAL_START,
            now_ms,
            dhcp: None,
        }
    }

    /// Configure a static IPv4 address and optional default gateway, replacing any DHCP lease.
    ///
    /// Prefix lengths above 32 are clamped to 32.
    pub fn set_ipv4(&mut self, addr: Ipv4Addr, prefix: u8, gateway: Option<Ipv4Addr>) {
        if let Some(handle) = self.dhcp.take() {
            drop(self.sockets.remove(handle));
        }
        let cidr = Ipv4Cidr::new(smol(addr), prefix.min(32));
        self.iface.update_ip_addrs(|addrs| {
            addrs.clear();
            let _ = addrs.push(IpCidr::Ipv4(cidr));
        });
        self.iface.routes_mut().remove_default_ipv4_route();
        if let Some(gateway) = gateway {
            let _ = self
                .iface
                .routes_mut()
                .add_default_ipv4_route(smol(gateway));
        }
    }

    /// Enable the DHCP client and clear any static IPv4 configuration.
    ///
    /// Calling this more than once is a no-op. Lease changes are applied during [`Self::poll`].
    pub fn enable_dhcp(&mut self) {
        if self.dhcp.is_some() {
            return;
        }
        self.iface.update_ip_addrs(|addrs| addrs.clear());
        self.iface.routes_mut().remove_default_ipv4_route();
        self.dhcp = Some(self.sockets.add(dhcpv4::Socket::new()));
    }

    /// Listen for TCP connections on `port`.
    ///
    /// `backlog` is the number of completed handshakes that may wait for [`Self::tcp_accept`];
    /// zero is treated as one. The limit is enforced by a pool of smoltcp listening sockets.
    pub fn tcp_listen(&mut self, port: u16, backlog: usize) -> Result<SocketId, Errno> {
        if port == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        if self.listener_on_port(port) {
            return Err(Errno::EADDRINUSE);
        }
        let limit = backlog.max(1);
        let mut pool = Vec::with_capacity(limit);
        for _ in 0..limit {
            pool.push(self.add_listener_socket(port)?);
        }
        let id = self.alloc_id();
        self.records.insert(
            id.0,
            Record {
                handle: None,
                kind: Kind::Listener(Listener {
                    port,
                    limit,
                    pool,
                    pending: Vec::new(),
                }),
                prev_readable: false,
                prev_writable: false,
                closed_reported: false,
                closed: false,
            },
        );
        Ok(id)
    }

    /// Take one completed connection from a listener.
    pub fn tcp_accept(&mut self, listener: SocketId) -> Result<Option<SocketId>, Errno> {
        match self.records.get(&listener.0) {
            None => return Err(Errno::EBADF),
            Some(record) if record.closed => return Err(Errno::EBADF),
            Some(record) if !matches!(record.kind, Kind::Listener(_)) => {
                return Err(Errno::ENOTSOCK);
            }
            Some(_) => {}
        }

        loop {
            let next = {
                let record = self.records.get_mut(&listener.0).ok_or(Errno::EBADF)?;
                let Kind::Listener(listener_state) = &mut record.kind else {
                    return Err(Errno::ENOTSOCK);
                };
                match listener_state.pending.first().copied() {
                    Some(id) => id,
                    None => return Ok(None),
                }
            };

            let usable = match self.records.get(&next.0) {
                None => false,
                Some(record) if record.closed => false,
                Some(record) => {
                    let handle = record.handle.ok_or(Errno::ENOTSOCK)?;
                    self.sockets.get::<tcp::Socket>(handle).state() != tcp::State::Closed
                }
            };

            if usable {
                self.remove_pending(listener.0, next);
                self.replenish_listener(listener.0);
                return Ok(Some(next));
            }

            self.remove_pending(listener.0, next);
            if let Some(record) = self.records.remove(&next.0)
                && let Some(handle) = record.handle
            {
                drop(self.sockets.remove(handle));
            }
            self.replenish_listener(listener.0);
        }
    }

    /// Start a TCP connection to `remote`; `local_port` defaults to an ephemeral port.
    pub fn tcp_connect(
        &mut self,
        remote: (Ipv4Addr, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        let (remote_addr, remote_port) = remote;
        if remote_port == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        let local_port = match local_port {
            Some(0) => return Err(Errno::EADDRNOTAVAIL),
            Some(port) if self.tcp_local_port_in_use(port) => return Err(Errno::EADDRINUSE),
            Some(port) => port,
            None => self.alloc_ephemeral_port()?,
        };

        let mut socket = new_tcp_socket();
        socket
            .connect(
                self.iface.context(),
                (IpAddress::Ipv4(smol(remote_addr)), remote_port),
                local_port,
            )
            .map_err(|_| Errno::EADDRNOTAVAIL)?;
        let handle = self.sockets.add(socket);
        let id = self.alloc_id();
        self.records.insert(
            id.0,
            Record {
                handle: Some(handle),
                kind: Kind::Tcp(TcpRecord {
                    local_port,
                    last_state: tcp::State::SynSent,
                    started_ms: self.now_ms,
                    ever_established: false,
                    locally_closed: false,
                }),
                prev_readable: false,
                prev_writable: false,
                closed_reported: false,
                closed: false,
            },
        );
        Ok(id)
    }

    /// Queue bytes on a TCP connection, or a datagram on a bound UDP socket.
    ///
    /// UDP sockets need a destination; use [`Self::udp_send_to`] for them. Returns
    /// [`Errno::EAGAIN`] when the transmit buffer is full.
    pub fn send(&mut self, s: SocketId, data: &[u8]) -> Result<usize, Errno> {
        let record = self.record(s)?;
        match &record.kind {
            Kind::Tcp(_) => {
                let handle = record.handle.ok_or(Errno::ENOTSOCK)?;
                let socket = self.sockets.get_mut::<tcp::Socket>(handle);
                if !socket.may_send() {
                    return Err(Errno::ENOTCONN);
                }
                match socket.send_slice(data) {
                    Ok(0) => Err(Errno::EAGAIN),
                    Ok(length) => Ok(length),
                    Err(_) => Err(Errno::ENOTCONN),
                }
            }
            Kind::Udp { .. } | Kind::Listener(_) => Err(Errno::ENOTCONN),
        }
    }

    /// Read bytes from a TCP connection or a datagram from a UDP socket.
    ///
    /// TCP returns `Ok(0)` after the peer's orderly close. An empty non-closed socket returns
    /// [`Errno::EAGAIN`]. UDP discards the source address; use [`Self::udp_recv_from`] for it.
    pub fn recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno> {
        let record = self.record(s)?;
        match &record.kind {
            Kind::Tcp(_) => {
                let handle = record.handle.ok_or(Errno::ENOTSOCK)?;
                let socket = self.sockets.get_mut::<tcp::Socket>(handle);
                if buf.is_empty() {
                    return Ok(0);
                }
                match socket.recv_slice(buf) {
                    Ok(0) => {
                        if socket.may_recv() {
                            Err(Errno::EAGAIN)
                        } else {
                            Ok(0)
                        }
                    }
                    Ok(length) => Ok(length),
                    Err(tcp::RecvError::Finished) => Ok(0),
                    Err(_) => Err(Errno::ENOTCONN),
                }
            }
            Kind::Udp { .. } => {
                let handle = record.handle.ok_or(Errno::ENOTSOCK)?;
                let socket = self.sockets.get_mut::<udp::Socket>(handle);
                match socket.recv_slice(buf) {
                    Ok((length, _)) => Ok(length),
                    Err(udp::RecvError::Exhausted) => Err(Errno::EAGAIN),
                    Err(udp::RecvError::Truncated) => Err(Errno::EINVAL),
                }
            }
            Kind::Listener(_) => Err(Errno::ENOTCONN),
        }
    }

    /// Close the transmit half of a TCP connection.
    pub fn shutdown_write(&mut self, s: SocketId) -> Result<(), Errno> {
        let (handle, is_tcp) = match self.records.get(&s.0) {
            Some(record) if !record.closed => (record.handle, matches!(record.kind, Kind::Tcp(_))),
            _ => return Err(Errno::EBADF),
        };
        if !is_tcp {
            return Err(Errno::ENOTCONN);
        }
        let handle = handle.ok_or(Errno::ENOTSOCK)?;
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        if !socket.is_open() {
            return Err(Errno::ENOTCONN);
        }
        socket.close();
        if let Some(record) = self.records.get_mut(&s.0)
            && let Kind::Tcp(tcp_record) = &mut record.kind
        {
            tcp_record.locally_closed = true;
        }
        Ok(())
    }

    /// Close a socket.
    ///
    /// TCP closes gracefully: the socket stays in the set until the FIN handshake completes, and
    /// the id becomes invalid immediately. UDP sockets and listeners are removed immediately;
    /// unaccepted connections of a closed listener keep their own ids.
    pub fn close(&mut self, s: SocketId) -> Result<(), Errno> {
        enum Action {
            Tcp(Option<SocketHandle>),
            Remove(Option<SocketHandle>),
            Listener(Vec<SocketHandle>),
        }

        let action = {
            let record = match self.records.get_mut(&s.0) {
                Some(record) if !record.closed => record,
                _ => return Err(Errno::EBADF),
            };
            match &mut record.kind {
                Kind::Tcp(tcp_record) => {
                    tcp_record.locally_closed = true;
                    record.closed = true;
                    record.closed_reported = true;
                    Action::Tcp(record.handle)
                }
                Kind::Udp { .. } => {
                    record.closed = true;
                    Action::Remove(record.handle)
                }
                Kind::Listener(listener) => {
                    record.closed = true;
                    Action::Listener(core::mem::take(&mut listener.pool))
                }
            }
        };

        match action {
            Action::Tcp(Some(handle)) => {
                let socket = self.sockets.get_mut::<tcp::Socket>(handle);
                if socket.is_open() {
                    socket.close();
                }
            }
            Action::Tcp(None) => {}
            Action::Remove(handle) => {
                if let Some(handle) = handle {
                    drop(self.sockets.remove(handle));
                }
                self.records.remove(&s.0);
            }
            Action::Listener(handles) => {
                for handle in handles {
                    drop(self.sockets.remove(handle));
                }
                self.records.remove(&s.0);
            }
        }
        Ok(())
    }

    /// Bind a UDP socket on `port`; `port` zero is rejected with [`Errno::EADDRNOTAVAIL`].
    pub fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno> {
        if port == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        if self.udp_port_in_use(port) {
            return Err(Errno::EADDRINUSE);
        }
        let mut socket = new_udp_socket();
        socket.bind(port).map_err(|_| Errno::EADDRNOTAVAIL)?;
        let handle = self.sockets.add(socket);
        let id = self.alloc_id();
        self.records.insert(
            id.0,
            Record {
                handle: Some(handle),
                kind: Kind::Udp { local_port: port },
                prev_readable: false,
                prev_writable: false,
                closed_reported: false,
                closed: false,
            },
        );
        Ok(id)
    }

    /// Send a UDP datagram to `to`. Returns [`Errno::EAGAIN`] when the transmit buffer is full.
    pub fn udp_send_to(
        &mut self,
        s: SocketId,
        data: &[u8],
        to: (Ipv4Addr, u16),
    ) -> Result<usize, Errno> {
        if to.1 == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        let record = self.record(s)?;
        let handle = match record.kind {
            Kind::Udp { .. } => record.handle.ok_or(Errno::ENOTSOCK)?,
            Kind::Tcp(_) | Kind::Listener(_) => return Err(Errno::ENOTSOCK),
        };
        let socket = self.sockets.get_mut::<udp::Socket>(handle);
        let endpoint = IpEndpoint::new(IpAddress::Ipv4(smol(to.0)), to.1);
        match socket.send_slice(data, endpoint) {
            Ok(()) => Ok(data.len()),
            Err(udp::SendError::BufferFull) => Err(Errno::EAGAIN),
            Err(udp::SendError::Unaddressable) => Err(Errno::EADDRNOTAVAIL),
        }
    }

    /// Receive a UDP datagram and its source address.
    pub fn udp_recv_from(
        &mut self,
        s: SocketId,
        buf: &mut [u8],
    ) -> Result<(usize, (Ipv4Addr, u16)), Errno> {
        let record = self.record(s)?;
        let handle = match record.kind {
            Kind::Udp { .. } => record.handle.ok_or(Errno::ENOTSOCK)?,
            Kind::Tcp(_) | Kind::Listener(_) => return Err(Errno::ENOTSOCK),
        };
        let socket = self.sockets.get_mut::<udp::Socket>(handle);
        let (length, metadata) = socket.recv_slice(buf).map_err(|error| match error {
            udp::RecvError::Exhausted => Errno::EAGAIN,
            udp::RecvError::Truncated => Errno::EINVAL,
        })?;
        let addr = match metadata.endpoint.addr {
            IpAddress::Ipv4(addr) => addr,
            #[allow(unreachable_patterns)]
            _ => return Err(Errno::EAFNOSUPPORT),
        };
        Ok((length, (ours(addr), metadata.endpoint.port)))
    }

    /// Drive the interface, apply DHCP updates and return readiness transitions since the last call.
    pub fn poll(&mut self, now_ms: i64) -> Vec<Event> {
        self.now_ms = now_ms;
        let now = Instant::from_millis(now_ms);
        let _ = self.iface.poll(now, &mut self.device, &mut self.sockets);
        self.apply_dhcp();

        let mut events = Vec::new();
        self.accept_completed_handshakes(now_ms, &mut events);
        self.collect_socket_events(now_ms, &mut events);
        self.reap_closed();
        events
    }

    /// Return how long the caller may sleep before the next [`Self::poll`], in milliseconds.
    ///
    /// The link's own propagation delay is not part of smoltcp's timer; a device that delays
    /// frames should be polled at least as often as its configured delay.
    pub fn next_poll_delay_ms(&mut self, now_ms: i64) -> Option<u64> {
        self.iface
            .poll_delay(Instant::from_millis(now_ms), &self.sockets)
            .map(|delay| delay.total_millis())
    }

    fn alloc_id(&mut self) -> SocketId {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        SocketId(id)
    }

    fn add_listener_socket(&mut self, port: u16) -> Result<SocketHandle, Errno> {
        let mut socket = new_tcp_socket();
        socket.listen(port).map_err(|_| Errno::EADDRNOTAVAIL)?;
        Ok(self.sockets.add(socket))
    }

    fn listener_on_port(&self, port: u16) -> bool {
        self.records.values().any(|record| match &record.kind {
            Kind::Listener(listener) => !record.closed && listener.port == port,
            _ => false,
        })
    }

    fn udp_port_in_use(&self, port: u16) -> bool {
        self.records.values().any(|record| match &record.kind {
            Kind::Udp { local_port } => !record.closed && *local_port == port,
            _ => false,
        })
    }

    fn tcp_local_port_in_use(&self, port: u16) -> bool {
        self.records.values().any(|record| match &record.kind {
            Kind::Listener(listener) => listener.port == port,
            Kind::Tcp(tcp_record) => {
                tcp_record.local_port == port
                    && record.handle.is_some_and(|handle| {
                        self.sockets.get::<tcp::Socket>(handle).state() != tcp::State::Closed
                    })
            }
            Kind::Udp { .. } => false,
        })
    }

    fn alloc_ephemeral_port(&mut self) -> Result<u16, Errno> {
        for _ in 0..EPHEMERAL_COUNT {
            let port = self.next_ephemeral;
            self.next_ephemeral = if port == u16::MAX {
                EPHEMERAL_START
            } else {
                port + 1
            };
            if !self.tcp_local_port_in_use(port) {
                return Ok(port);
            }
        }
        Err(Errno::EADDRNOTAVAIL)
    }

    fn remove_pending(&mut self, listener_id: u32, conn: SocketId) {
        if let Some(record) = self.records.get_mut(&listener_id)
            && let Kind::Listener(listener) = &mut record.kind
        {
            listener.pending.retain(|id| *id != conn);
        }
    }

    fn replenish_listener(&mut self, listener_id: u32) {
        let port = {
            let Some(record) = self.records.get(&listener_id) else {
                return;
            };
            let Kind::Listener(listener) = &record.kind else {
                return;
            };
            if record.closed || listener.pool.len() >= listener.limit {
                return;
            }
            listener.port
        };
        if let Ok(handle) = self.add_listener_socket(port)
            && let Some(record) = self.records.get_mut(&listener_id)
            && let Kind::Listener(listener) = &mut record.kind
        {
            listener.pool.push(handle);
        }
    }

    fn remove_pool_handle(&mut self, listener_id: u32, handle: SocketHandle) {
        if let Some(record) = self.records.get_mut(&listener_id)
            && let Kind::Listener(listener) = &mut record.kind
        {
            listener.pool.retain(|candidate| *candidate != handle);
        }
    }

    fn accept_completed_handshakes(&mut self, now_ms: i64, events: &mut Vec<Event>) {
        let listeners: Vec<u32> = self
            .records
            .iter()
            .filter(|(_, record)| !record.closed && matches!(record.kind, Kind::Listener(_)))
            .map(|(&id, _)| id)
            .collect();

        for listener_id in listeners {
            let (port, handles) = {
                let Some(record) = self.records.get(&listener_id) else {
                    continue;
                };
                let Kind::Listener(listener) = &record.kind else {
                    continue;
                };
                (listener.port, listener.pool.clone())
            };

            for handle in handles {
                let state = self.sockets.get::<tcp::Socket>(handle).state();
                match state {
                    tcp::State::Listen | tcp::State::SynReceived => {}
                    tcp::State::Closed => {
                        self.remove_pool_handle(listener_id, handle);
                        drop(self.sockets.remove(handle));
                        self.replenish_listener(listener_id);
                    }
                    _ => {
                        self.remove_pool_handle(listener_id, handle);
                        let conn = self.alloc_id();
                        self.records.insert(
                            conn.0,
                            Record {
                                handle: Some(handle),
                                kind: Kind::Tcp(TcpRecord {
                                    local_port: port,
                                    last_state: state,
                                    started_ms: now_ms,
                                    ever_established: true,
                                    locally_closed: false,
                                }),
                                prev_readable: false,
                                prev_writable: false,
                                closed_reported: false,
                                closed: false,
                            },
                        );
                        if let Some(record) = self.records.get_mut(&listener_id)
                            && let Kind::Listener(listener) = &mut record.kind
                        {
                            listener.pending.push(conn);
                        }
                        events.push(Event::Accepted {
                            listener: SocketId(listener_id),
                            conn,
                        });
                    }
                }
            }
        }
    }

    fn collect_socket_events(&mut self, now_ms: i64, events: &mut Vec<Event>) {
        let sockets: Vec<(u32, SocketHandle, bool)> = self
            .records
            .iter()
            .filter(|(_, record)| !record.closed && !matches!(record.kind, Kind::Listener(_)))
            .filter_map(|(&id, record)| {
                record
                    .handle
                    .map(|handle| (id, handle, matches!(record.kind, Kind::Tcp(_))))
            })
            .collect();

        for (id, handle, is_tcp) in sockets {
            if is_tcp {
                self.poll_tcp(id, handle, now_ms, events);
            } else {
                self.poll_udp(id, handle, events);
            }
        }
    }

    fn poll_tcp(&mut self, id: u32, handle: SocketHandle, now_ms: i64, events: &mut Vec<Event>) {
        let timed_out = self
            .records
            .get(&id)
            .is_some_and(|record| match &record.kind {
                Kind::Tcp(tcp_record) => {
                    tcp_record.last_state == tcp::State::SynSent
                        && now_ms.saturating_sub(tcp_record.started_ms) >= CONNECT_TIMEOUT_MS
                }
                _ => false,
            });
        if timed_out {
            self.sockets.get_mut::<tcp::Socket>(handle).abort();
        }

        let (state, readable, writable, peer_eof) = {
            let socket = self.sockets.get_mut::<tcp::Socket>(handle);
            let state = socket.state();
            let readable = socket.can_recv();
            let writable = socket.can_send();
            let peer_eof = !socket.can_recv()
                && matches!(
                    state,
                    tcp::State::CloseWait
                        | tcp::State::LastAck
                        | tcp::State::Closing
                        | tcp::State::TimeWait
                );
            (state, readable, writable, peer_eof)
        };

        let Some(record) = self.records.get_mut(&id) else {
            return;
        };
        let Kind::Tcp(tcp_record) = &mut record.kind else {
            return;
        };

        if !matches!(
            state,
            tcp::State::Closed | tcp::State::Listen | tcp::State::SynSent | tcp::State::SynReceived
        ) {
            tcp_record.ever_established = true;
        }
        if readable && !record.prev_readable {
            events.push(Event::Readable(SocketId(id)));
        }
        if writable && !record.prev_writable {
            events.push(Event::Writable(SocketId(id)));
        }
        record.prev_readable = readable;
        record.prev_writable = writable;

        if state == tcp::State::Closed && !record.closed_reported {
            record.closed_reported = true;
            if timed_out {
                events.push(Event::Error(SocketId(id), Errno::ETIMEDOUT));
            } else if !tcp_record.ever_established && !tcp_record.locally_closed {
                events.push(Event::Error(SocketId(id), Errno::ECONNREFUSED));
            } else if tcp_record.ever_established && !tcp_record.locally_closed {
                events.push(Event::Error(SocketId(id), Errno::ECONNRESET));
            } else {
                events.push(Event::Closed(SocketId(id)));
            }
        } else if peer_eof && !record.closed_reported {
            record.closed_reported = true;
            events.push(Event::Closed(SocketId(id)));
        }

        tcp_record.last_state = state;
    }

    fn poll_udp(&mut self, id: u32, handle: SocketHandle, events: &mut Vec<Event>) {
        let (readable, writable) = {
            let socket = self.sockets.get_mut::<udp::Socket>(handle);
            (socket.can_recv(), socket.can_send())
        };
        let Some(record) = self.records.get_mut(&id) else {
            return;
        };
        if readable && !record.prev_readable {
            events.push(Event::Readable(SocketId(id)));
        }
        if writable && !record.prev_writable {
            events.push(Event::Writable(SocketId(id)));
        }
        record.prev_readable = readable;
        record.prev_writable = writable;
    }

    fn reap_closed(&mut self) {
        let closed: Vec<u32> = self
            .records
            .iter()
            .filter(|(_, record)| record.closed)
            .map(|(&id, _)| id)
            .collect();
        for id in closed {
            let remove = match self.records.get(&id) {
                None => false,
                Some(record) => match &record.kind {
                    Kind::Tcp(_) => record.handle.is_some_and(|handle| {
                        self.sockets.get::<tcp::Socket>(handle).state() == tcp::State::Closed
                    }),
                    Kind::Udp { .. } | Kind::Listener(_) => true,
                },
            };
            if remove
                && let Some(record) = self.records.remove(&id)
                && let Some(handle) = record.handle
            {
                drop(self.sockets.remove(handle));
            }
        }
    }

    fn apply_dhcp(&mut self) {
        let Some(handle) = self.dhcp else {
            return;
        };
        let update = {
            let socket = self.sockets.get_mut::<dhcpv4::Socket>(handle);
            match socket.poll() {
                Some(dhcpv4::Event::Configured(config)) => {
                    Some(Some((config.address, config.router)))
                }
                Some(dhcpv4::Event::Deconfigured) => Some(None),
                None => None,
            }
        };
        match update {
            Some(Some((cidr, router))) => {
                self.iface.update_ip_addrs(|addrs| {
                    addrs.clear();
                    let _ = addrs.push(IpCidr::Ipv4(cidr));
                });
                self.iface.routes_mut().remove_default_ipv4_route();
                if let Some(gateway) = router {
                    let _ = self.iface.routes_mut().add_default_ipv4_route(gateway);
                }
            }
            Some(None) => {
                self.iface.update_ip_addrs(|addrs| addrs.clear());
                self.iface.routes_mut().remove_default_ipv4_route();
            }
            None => {}
        }
    }

    fn record(&self, s: SocketId) -> Result<&Record, Errno> {
        match self.records.get(&s.0) {
            Some(record) if !record.closed => Ok(record),
            _ => Err(Errno::EBADF),
        }
    }
}

fn new_tcp_socket() -> tcp::Socket<'static> {
    tcp::Socket::new(
        tcp::SocketBuffer::new(vec![0; TCP_BUFFER_SIZE]),
        tcp::SocketBuffer::new(vec![0; TCP_BUFFER_SIZE]),
    )
}

fn new_udp_socket() -> udp::Socket<'static> {
    udp::Socket::new(
        udp::PacketBuffer::new(
            vec![udp::PacketMetadata::EMPTY; UDP_METADATA_SLOTS],
            vec![0; UDP_PAYLOAD_SIZE],
        ),
        udp::PacketBuffer::new(
            vec![udp::PacketMetadata::EMPTY; UDP_METADATA_SLOTS],
            vec![0; UDP_PAYLOAD_SIZE],
        ),
    )
}

fn random_seed(mac: [u8; 6], now_ms: i64) -> u64 {
    let mut seed = 0x9e37_79b9_7f4a_7c15 ^ now_ms as u64;
    for byte in mac {
        seed = seed.rotate_left(9) ^ u64::from(byte).wrapping_mul(0x100_0000_01b3);
    }
    seed
}

impl<D: Device> StackBackend for SmoltcpBackend<D> {
    type Device = D;

    fn set_ipv4(&mut self, addr: Ipv4Addr, prefix: u8, gateway: Option<Ipv4Addr>) {
        SmoltcpBackend::set_ipv4(self, addr, prefix, gateway);
    }

    fn enable_dhcp(&mut self) {
        SmoltcpBackend::enable_dhcp(self);
    }

    fn tcp_listen(&mut self, port: u16, backlog: usize) -> Result<SocketId, Errno> {
        SmoltcpBackend::tcp_listen(self, port, backlog)
    }

    fn tcp_accept(&mut self, listener: SocketId) -> Result<Option<SocketId>, Errno> {
        SmoltcpBackend::tcp_accept(self, listener)
    }

    fn tcp_connect(
        &mut self,
        remote: (Ipv4Addr, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        SmoltcpBackend::tcp_connect(self, remote, local_port)
    }

    fn send(&mut self, s: SocketId, data: &[u8]) -> Result<usize, Errno> {
        SmoltcpBackend::send(self, s, data)
    }

    fn recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno> {
        SmoltcpBackend::recv(self, s, buf)
    }

    fn shutdown_write(&mut self, s: SocketId) -> Result<(), Errno> {
        SmoltcpBackend::shutdown_write(self, s)
    }

    fn close(&mut self, s: SocketId) -> Result<(), Errno> {
        SmoltcpBackend::close(self, s)
    }

    fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno> {
        SmoltcpBackend::udp_bind(self, port)
    }

    fn udp_send_to(
        &mut self,
        s: SocketId,
        data: &[u8],
        to: (Ipv4Addr, u16),
    ) -> Result<usize, Errno> {
        SmoltcpBackend::udp_send_to(self, s, data, to)
    }

    fn udp_recv_from(
        &mut self,
        s: SocketId,
        buf: &mut [u8],
    ) -> Result<(usize, (Ipv4Addr, u16)), Errno> {
        SmoltcpBackend::udp_recv_from(self, s, buf)
    }

    fn poll(&mut self, now_ms: i64) -> Vec<Event> {
        SmoltcpBackend::poll(self, now_ms)
    }

    fn next_poll_delay_ms(&mut self, now_ms: i64) -> Option<u64> {
        SmoltcpBackend::next_poll_delay_ms(self, now_ms)
    }
}

fn smol(addr: Ipv4Addr) -> smoltcp::wire::Ipv4Address {
    let [a, b, c, d] = addr.octets();
    smoltcp::wire::Ipv4Address::new(a, b, c, d)
}

fn ours(addr: smoltcp::wire::Ipv4Address) -> Ipv4Addr {
    Ipv4Addr::from_octets(addr.octets())
}
