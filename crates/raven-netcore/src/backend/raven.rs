// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The native `raven-tcpip` implementation of [`StackBackend`].

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::Cell;

use crate::backend::StackBackend;
use crate::backend::counting::{CountingDevice, SharedCounters};
use crate::{
    CoreSocket, Event, InterfaceCounters, Ipv4Addr, RouteEntry, SocketId, SocketState,
    SocketTransport,
};
use raven_abi::errno::Errno;
use raven_tcpip::{self as native, Device, Instant, IpAddr, Ipv4Config, Stack, StackConfig};

/// The native stack behind [`StackBackend`].
pub struct RavenTcpipBackend<D: Device> {
    stack: Stack<CountingDevice<D>>,
    counters: SharedCounters,
    rx_drops: u64,
    closed: BTreeSet<u32>,
    notified_closed: BTreeSet<u32>,
    accepted: BTreeSet<u32>,
    listeners: BTreeMap<u16, usize>,
    udp_ports: BTreeMap<u32, u16>,
    udp_remotes: BTreeMap<u32, (Ipv4Addr, u16)>,
    icmp_ids: BTreeMap<u32, (u16, Option<Ipv4Addr>)>,
    pending_udp: Vec<(SocketId, Vec<u8>, (Ipv4Addr, u16))>,
    pending_udp_bytes: usize,
    options: BTreeMap<(u32, u32), i32>,
}

impl<D: Device> RavenTcpipBackend<D> {
    /// Create a native backend over `device` with Ethernet address `mac`.
    ///
    /// Time is a millisecond epoch: `now_ms` is converted directly to C15's
    /// `Instant` and must never decrease.
    pub fn new(device: D, mac: [u8; 6], now_ms: i64) -> Self {
        let seed = u64::from_le_bytes([mac[0], mac[1], mac[2], mac[3], mac[4], 0x5a, 0, 0]);
        let counters = Rc::new(Cell::new(InterfaceCounters::default()));
        let stack = Stack::new(
            CountingDevice::new(device, Rc::clone(&counters)),
            StackConfig {
                hardware_addr: Some(mac),
                ..StackConfig::default()
            },
            seed,
            Instant::from_millis(now_ms),
        );
        Self {
            stack,
            counters,
            rx_drops: 0,
            closed: BTreeSet::new(),
            notified_closed: BTreeSet::new(),
            accepted: BTreeSet::new(),
            listeners: BTreeMap::new(),
            udp_ports: BTreeMap::new(),
            udp_remotes: BTreeMap::new(),
            icmp_ids: BTreeMap::new(),
            pending_udp: Vec::new(),
            pending_udp_bytes: 0,
            options: BTreeMap::new(),
        }
    }

    /// A send on a connection the peer already tore down reports `ENOTCONN` (the socket layer's
    /// long-standing contract), while an id the stack never issued stays `EBADF`.
    fn dead_connection_error(&self, s: SocketId, error: Errno) -> Errno {
        if error == Errno::EBADF && self.stack.socket_info(Self::native(s)).is_ok() {
            Errno::ENOTCONN
        } else {
            error
        }
    }

    fn native(socket: SocketId) -> native::SocketId {
        native::SocketId(socket.0)
    }

    fn ours(socket: native::SocketId) -> SocketId {
        SocketId(socket.0)
    }

    fn flush_pending_udp(&mut self) {
        let mut pending = core::mem::take(&mut self.pending_udp);
        let mut again = Vec::new();
        for (socket, data, to) in pending.drain(..) {
            let destination = (IpAddr::V4(native::Ipv4Addr::new(to.0.octets())), to.1);
            match self
                .stack
                .udp_send_to(Self::native(socket), &data, destination)
            {
                Ok(_) => {
                    self.pending_udp_bytes = self.pending_udp_bytes.saturating_sub(data.len());
                }
                Err(Errno::EAGAIN) => again.push((socket, data, to)),
                Err(_) => {
                    self.pending_udp_bytes = self.pending_udp_bytes.saturating_sub(data.len());
                }
            }
        }
        self.pending_udp = again;
    }

    fn enforce_backlog(&mut self) {
        let listeners: Vec<(u16, usize)> = self
            .listeners
            .iter()
            .map(|(port, backlog)| (*port, *backlog))
            .collect();
        for (port, backlog) in listeners {
            loop {
                let mut pending: Vec<u32> =
                    self.stack
                        .sockets()
                        .filter(|id| {
                            !self.accepted.contains(&id.0)
                                && !self.closed.contains(&id.0)
                                && self.stack.socket_info(*id).ok().is_some_and(|info| {
                                    info.local.1 == port && info.remote.is_some()
                                })
                        })
                        .map(|id| id.0)
                        .collect();
                if pending.len() <= backlog {
                    break;
                }
                pending.sort_unstable();
                if let Some(victim) = pending.last().copied() {
                    let _ = self.stack.abort(native::SocketId(victim));
                    self.closed.insert(victim);
                } else {
                    break;
                }
            }
        }
    }

    fn map_event(event: native::Event) -> Event {
        match event {
            native::Event::Readable(socket) => Event::Readable(Self::ours(socket)),
            native::Event::Writable(socket) => Event::Writable(Self::ours(socket)),
            native::Event::Closed(socket) => Event::Closed(Self::ours(socket)),
            native::Event::Error(socket, error) => Event::Error(
                Self::ours(socket),
                if error == Errno::ECONNRESET {
                    Errno::ECONNREFUSED
                } else {
                    error
                },
            ),
            native::Event::Accepted { listener, conn } => Event::Accepted {
                listener: Self::ours(listener),
                conn: Self::ours(conn),
            },
        }
    }
}

impl<D: Device> StackBackend for RavenTcpipBackend<D> {
    type Device = D;

    fn set_ipv4(&mut self, addr: Ipv4Addr, prefix: u8, gateway: Option<Ipv4Addr>) {
        let _ = self.stack.set_ipv4(Ipv4Config {
            addr: native::Ipv4Addr::new(addr.octets()),
            prefix_len: prefix,
            gateway: gateway.map(|gateway| native::Ipv4Addr::new(gateway.octets())),
        });
    }

    fn enable_dhcp(&mut self) {
        let _ = self.stack.dhcp_enable();
    }

    fn try_enable_dhcp(&mut self) -> Result<(), Errno> {
        self.stack.dhcp_enable()
    }

    fn dhcp_lease(&self) -> Option<crate::DhcpLease> {
        let lease = self.stack.dhcp_lease()?;
        let timers = self.stack.dhcp_timers();
        Some(crate::DhcpLease {
            address: Ipv4Addr::from_octets(lease.address.octets()),
            prefix_len: lease.prefix_len,
            router: lease
                .router
                .map(|router| Ipv4Addr::from_octets(router.octets())),
            dns: lease
                .dns
                .iter()
                .map(|server| Ipv4Addr::from_octets(server.octets()))
                .collect(),
            renew_at_ms: timers.map(|timers| timers.renew_at),
            rebind_at_ms: timers.map(|timers| timers.rebind_at),
            expires_at_ms: timers.map(|timers| timers.expires_at),
        })
    }

    fn routes(&self) -> Vec<RouteEntry> {
        self.stack
            .routes()
            .entries()
            .iter()
            .map(|route| RouteEntry {
                dest: Ipv4Addr::from_octets(canonical_prefix(route.prefix, route.prefix_len)),
                prefix_len: route.prefix_len,
                gateway: route
                    .gateway
                    .map(|gateway| Ipv4Addr::from_octets(gateway.octets())),
                mtu: route.mtu,
            })
            .collect()
    }

    fn add_route(&mut self, route: RouteEntry) -> Result<(), Errno> {
        if route.prefix_len > 32 || route.mtu == 0 {
            return Err(Errno::EINVAL);
        }
        let native_dest = native::Ipv4Addr::new(route.dest.octets());
        self.stack.routes_mut().add(native::Route {
            prefix: native::Ipv4Addr::new(canonical_prefix(native_dest, route.prefix_len)),
            prefix_len: route.prefix_len,
            gateway: route
                .gateway
                .map(|gateway| native::Ipv4Addr::new(gateway.octets())),
            mtu: route.mtu,
        })
    }

    fn delete_route(&mut self, dest: Ipv4Addr, prefix_len: u8) -> Result<(), Errno> {
        if prefix_len > 32 {
            return Err(Errno::EINVAL);
        }
        let native_dest = native::Ipv4Addr::new(dest.octets());
        if self.stack.routes_mut().remove(
            native::Ipv4Addr::new(canonical_prefix(native_dest, prefix_len)),
            prefix_len,
        ) {
            Ok(())
        } else {
            Err(Errno::ENOENT)
        }
    }

    fn socket_table(&self) -> Vec<CoreSocket> {
        let local_addr = self
            .stack
            .ipv4_config()
            .map(|config| Ipv4Addr::from_octets(config.addr.octets()))
            .unwrap_or(Ipv4Addr::UNSPECIFIED);
        let mut entries = Vec::new();
        for socket in self.stack.sockets() {
            if let Ok(info) = self.stack.socket_info(socket) {
                let (transport, state) = if info.state == native::TcpState::Listen {
                    (SocketTransport::Tcp, SocketState::Listen)
                } else {
                    (SocketTransport::Tcp, map_tcp_state(info.state))
                };
                entries.push(CoreSocket {
                    id: Self::ours(socket),
                    transport,
                    state,
                    local: endpoint(info.local),
                    remote: info.remote.and_then(endpoint),
                    rx_queue: info.recv_queue.min(u32::MAX as usize) as u32,
                    tx_queue: info.send_queue.min(u32::MAX as usize) as u32,
                });
            } else if let Some(port) = self.udp_ports.get(&socket.0) {
                entries.push(CoreSocket {
                    id: Self::ours(socket),
                    transport: SocketTransport::Udp,
                    state: SocketState::Udp,
                    local: Some((local_addr, *port)),
                    remote: self.udp_remotes.get(&socket.0).copied(),
                    rx_queue: 0,
                    tx_queue: 0,
                });
            } else if let Some((_, target)) = self.icmp_ids.get(&socket.0) {
                entries.push(CoreSocket {
                    id: Self::ours(socket),
                    transport: SocketTransport::Icmp,
                    state: SocketState::Icmp,
                    local: None,
                    remote: target.map(|target| (target, 0)),
                    rx_queue: 0,
                    tx_queue: 0,
                });
            }
        }
        entries
    }

    fn interface_counters(&self) -> Option<InterfaceCounters> {
        let mut counters = self.counters.get();
        counters.rx_drops = counters.rx_drops.saturating_add(self.rx_drops);
        Some(counters)
    }

    fn icmp_bind(&mut self, id: u16) -> Result<SocketId, Errno> {
        let socket = self.stack.icmp_bind_echo(id).map(Self::ours)?;
        self.icmp_ids.insert(socket.0, (id, None));
        Ok(socket)
    }

    fn icmp_echo(
        &mut self,
        s: SocketId,
        to: Ipv4Addr,
        id: u16,
        seq: u16,
        payload: &[u8],
    ) -> Result<(), Errno> {
        let Some((bound, _)) = self.icmp_ids.get(&s.0) else {
            return Err(Errno::EBADF);
        };
        if *bound != id {
            return Err(Errno::EINVAL);
        }
        self.stack
            .icmp_echo_request(native::Ipv4Addr::new(to.octets()), id, seq, payload)?;
        if let Some(meta) = self.icmp_ids.get_mut(&s.0) {
            meta.1 = Some(to);
        }
        Ok(())
    }

    fn icmp_recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno> {
        if self.closed.contains(&s.0) {
            return Err(Errno::EBADF);
        }
        self.stack.recv(Self::native(s), buf)
    }

    fn tcp_listen(&mut self, port: u16, backlog: usize) -> Result<SocketId, Errno> {
        let socket = self.stack.tcp_listen(port, backlog).map(Self::ours)?;
        self.listeners.insert(port, backlog.max(1));
        let _ = self
            .stack
            .set_property("tcp.accept_queue_limit", backlog.max(1) as u64);
        Ok(socket)
    }

    fn tcp_accept(&mut self, listener: SocketId) -> Result<Option<SocketId>, Errno> {
        if self.closed.contains(&listener.0) {
            return Err(Errno::EBADF);
        }
        let accepted = self
            .stack
            .tcp_accept(Self::native(listener))
            .map(|socket| socket.map(Self::ours))?;
        if let Some(socket) = accepted {
            self.accepted.insert(socket.0);
        }
        Ok(accepted)
    }

    fn tcp_connect(
        &mut self,
        remote: (Ipv4Addr, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        let remote = (
            IpAddr::V4(native::Ipv4Addr::new(remote.0.octets())),
            remote.1,
        );
        self.stack.tcp_connect(remote, local_port).map(Self::ours)
    }

    fn send(&mut self, s: SocketId, data: &[u8]) -> Result<usize, Errno> {
        if self.closed.contains(&s.0) {
            return Err(Errno::EBADF);
        }
        self.stack
            .send(Self::native(s), data)
            .map_err(|error| self.dead_connection_error(s, map_wrong_type(error)))
    }

    fn recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno> {
        if self.closed.contains(&s.0) {
            return Err(Errno::EBADF);
        }
        self.stack
            .recv(Self::native(s), buf)
            .map_err(map_wrong_type)
    }

    fn shutdown_write(&mut self, s: SocketId) -> Result<(), Errno> {
        if self.closed.contains(&s.0) {
            return Err(Errno::EBADF);
        }
        self.stack
            .shutdown_write(Self::native(s))
            .map_err(map_wrong_type)
    }

    fn close(&mut self, s: SocketId) -> Result<(), Errno> {
        if self.closed.contains(&s.0) {
            return Err(Errno::EBADF);
        }
        self.stack.close(Self::native(s))?;
        self.closed.insert(s.0);
        self.options.retain(|(socket, _), _| *socket != s.0);
        self.udp_ports.remove(&s.0);
        self.udp_remotes.remove(&s.0);
        self.icmp_ids.remove(&s.0);
        Ok(())
    }

    fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno> {
        if port == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        let socket = self.stack.udp_bind(port).map(Self::ours)?;
        self.udp_ports.insert(socket.0, port);
        Ok(socket)
    }

    fn udp_send_to(
        &mut self,
        s: SocketId,
        data: &[u8],
        to: (Ipv4Addr, u16),
    ) -> Result<usize, Errno> {
        if self.closed.contains(&s.0) {
            return Err(Errno::EBADF);
        }
        let destination = (IpAddr::V4(native::Ipv4Addr::new(to.0.octets())), to.1);
        let result = match self.stack.udp_send_to(Self::native(s), data, destination) {
            Ok(length) => Ok(length),
            Err(Errno::EAGAIN) => {
                if self.pending_udp_bytes.saturating_add(data.len()) > 65_535 {
                    return Err(Errno::EAGAIN);
                }
                self.pending_udp_bytes = self.pending_udp_bytes.saturating_add(data.len());
                self.pending_udp.push((s, data.to_vec(), to));
                Ok(data.len())
            }
            Err(error) => Err(error),
        };
        if result.is_ok() {
            self.udp_remotes.insert(s.0, to);
        }
        result
    }

    fn udp_recv_from(
        &mut self,
        s: SocketId,
        buf: &mut [u8],
    ) -> Result<(usize, (Ipv4Addr, u16)), Errno> {
        if self.closed.contains(&s.0) {
            return Err(Errno::EBADF);
        }
        let (length, (source, port)) = self.stack.udp_recv_from(Self::native(s), buf)?;
        let IpAddr::V4(source) = source else {
            return Err(Errno::EAFNOSUPPORT);
        };
        Ok((length, (Ipv4Addr::from_octets(source.octets()), port)))
    }

    fn poll(&mut self, now_ms: i64) -> Vec<Event> {
        let result = self.stack.poll(Instant::from_millis(now_ms));
        self.rx_drops = self
            .rx_drops
            .saturating_add(u64::try_from(result.dropped).unwrap_or(u64::MAX));
        self.enforce_backlog();
        self.flush_pending_udp();
        let mut events: Vec<Event> = self
            .stack
            .take_events()
            .filter(|event| !self.closed.contains(&event_socket(*event).0))
            .map(Self::map_event)
            .collect();
        for socket in self.stack.sockets().collect::<Vec<_>>() {
            if self.closed.contains(&socket.0) || self.notified_closed.contains(&socket.0) {
                continue;
            }
            if let Ok(info) = self.stack.socket_info(socket)
                && matches!(
                    info.state,
                    native::TcpState::CloseWait
                        | native::TcpState::LastAck
                        | native::TcpState::TimeWait
                        | native::TcpState::Closed
                )
            {
                self.notified_closed.insert(socket.0);
                events.push(Event::Closed(Self::ours(socket)));
            }
        }
        events
    }

    fn next_poll_delay_ms(&mut self, now_ms: i64) -> Option<u64> {
        self.stack
            .next_poll_delay(Instant::from_millis(now_ms))
            .map(|duration| duration.as_millis())
    }
    fn local_endpoint(&self, s: SocketId) -> Option<(Ipv4Addr, u16)> {
        let info = self.stack.socket_info(Self::native(s)).ok()?;
        let (address, port) = info.local;
        match address {
            IpAddr::V4(address) => Some((Ipv4Addr::from_octets(address.octets()), port)),
            _ => None,
        }
    }

    fn peer_endpoint(&self, s: SocketId) -> Option<(Ipv4Addr, u16)> {
        let info = self.stack.socket_info(Self::native(s)).ok()?;
        let (address, port) = info.remote?;
        match address {
            IpAddr::V4(address) => Some((Ipv4Addr::from_octets(address.octets()), port)),
            _ => None,
        }
    }

    fn set_socket_option(&mut self, s: SocketId, option: u32, value: i32) -> Result<(), Errno> {
        if self.closed.contains(&s.0) {
            return Err(Errno::EBADF);
        }
        match option {
            // The native stack has no per-socket bind/buffer knob in this revision; retain the
            // requested values and apply TCP_NODELAY through the stack setter.
            1..=3 => {
                self.options.insert((s.0, option), value);
                Ok(())
            }
            4 => {
                self.stack.set_nodelay(Self::native(s), value != 0)?;
                self.options.insert((s.0, option), value);
                Ok(())
            }
            _ => Err(Errno::EINVAL),
        }
    }

    fn get_socket_option(&self, s: SocketId, option: u32) -> Result<i32, Errno> {
        if self.closed.contains(&s.0) {
            return Err(Errno::EBADF);
        }
        match option {
            1..=4 => Ok(*self.options.get(&(s.0, option)).unwrap_or(&0)),
            5 => Ok(0),
            _ => Err(Errno::EINVAL),
        }
    }
}

fn canonical_prefix(address: native::Ipv4Addr, prefix_len: u8) -> [u8; 4] {
    let mut octets = address.octets();
    if prefix_len == 0 {
        return [0; 4];
    }
    if prefix_len < 32 {
        let mask = u32::MAX << (32 - prefix_len);
        let value = u32::from_be_bytes(octets) & mask;
        octets = value.to_be_bytes();
    }
    octets
}

fn endpoint(value: (IpAddr, u16)) -> Option<(Ipv4Addr, u16)> {
    match value.0 {
        IpAddr::V4(address) => Some((Ipv4Addr::from_octets(address.octets()), value.1)),
        _ => None,
    }
}

fn map_tcp_state(state: native::TcpState) -> SocketState {
    match state {
        native::TcpState::Closed => SocketState::Closed,
        native::TcpState::Listen => SocketState::Listen,
        native::TcpState::SynSent => SocketState::SynSent,
        native::TcpState::SynReceived => SocketState::SynReceived,
        native::TcpState::Established => SocketState::Established,
        native::TcpState::FinWait1 => SocketState::FinWait1,
        native::TcpState::FinWait2 => SocketState::FinWait2,
        native::TcpState::CloseWait => SocketState::CloseWait,
        native::TcpState::Closing => SocketState::Closing,
        native::TcpState::LastAck => SocketState::LastAck,
        native::TcpState::TimeWait => SocketState::TimeWait,
    }
}

fn map_wrong_type(error: Errno) -> Errno {
    if error == Errno::ENOTSOCK {
        Errno::ENOTCONN
    } else {
        error
    }
}

fn event_socket(event: native::Event) -> SocketId {
    match event {
        native::Event::Readable(socket)
        | native::Event::Writable(socket)
        | native::Event::Closed(socket)
        | native::Event::Error(socket, _) => SocketId(socket.0),
        native::Event::Accepted { listener, .. } => SocketId(listener.0),
    }
}
