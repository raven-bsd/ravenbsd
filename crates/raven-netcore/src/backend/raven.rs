// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The native `raven-tcpip` implementation of [`StackBackend`].

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use crate::backend::StackBackend;
use crate::{Event, Ipv4Addr, SocketId};
use raven_abi::errno::Errno;
use raven_tcpip::{self as native, Device, Instant, IpAddr, Ipv4Config, Stack, StackConfig};

/// The native stack behind [`StackBackend`].
pub struct RavenTcpipBackend<D: Device> {
    stack: Stack<D>,
    closed: BTreeSet<u32>,
    notified_closed: BTreeSet<u32>,
    accepted: BTreeSet<u32>,
    listeners: BTreeMap<u16, usize>,
    pending_udp: Vec<(SocketId, Vec<u8>, (Ipv4Addr, u16))>,
    pending_udp_bytes: usize,
}

impl<D: Device> RavenTcpipBackend<D> {
    /// Create a native backend over `device` with Ethernet address `mac`.
    ///
    /// Time is a millisecond epoch: `now_ms` is converted directly to C15's
    /// `Instant` and must never decrease.
    pub fn new(device: D, mac: [u8; 6], now_ms: i64) -> Self {
        let seed = u64::from_le_bytes([mac[0], mac[1], mac[2], mac[3], mac[4], 0x5a, 0, 0]);
        Self {
            stack: Stack::new(
                device,
                StackConfig {
                    hardware_addr: Some(mac),
                    ..StackConfig::default()
                },
                seed,
                Instant::from_millis(now_ms),
            ),
            closed: BTreeSet::new(),
            notified_closed: BTreeSet::new(),
            accepted: BTreeSet::new(),
            listeners: BTreeMap::new(),
            pending_udp: Vec::new(),
            pending_udp_bytes: 0,
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
        // raven-tcpip has no DHCP client: the call changes nothing and arms no timer. Callers
        // that need to know use `try_enable_dhcp`, which reports EOPNOTSUPP.
    }

    fn try_enable_dhcp(&mut self) -> Result<(), Errno> {
        Err(Errno::EOPNOTSUPP)
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
        Ok(())
    }

    fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno> {
        if port == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        self.stack.udp_bind(port).map(Self::ours)
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
        match self.stack.udp_send_to(Self::native(s), data, destination) {
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
        }
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
        let _ = self.stack.poll(Instant::from_millis(now_ms));
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
