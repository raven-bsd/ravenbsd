// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The resolver's `DnsIo`, `Clock` and `Entropy` implementations over `Net`.
use crate::ErrnoExt;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::socket::{SOCK_DGRAM, SOCK_STREAM};
use raven_libc_resolv::{
    Clock, DnsIo, Entropy, Family, Interest, InterestKind, IpAddr, Recv, SockAddr, TcpSock, UdpSock,
};
use raven_proto_net::{Call, Endpoint};

use crate::msg::{bare_path, socket_path};
use crate::{Net, NetBackend, NetEnv, Sock, SocketFds, UnixBackend, WaitItem};

/// Per-`Net` DNS socket bookkeeping.
#[derive(Default)]
pub struct DnsState {
    next: u32,
    udp: BTreeMap<u32, UdpGroup>,
    tcp: BTreeMap<u32, TcpEntry>,
}

struct UdpGroup {
    members: Vec<(Endpoint, Sock)>,
    bare: Option<Sock>,
}

struct TcpEntry {
    sock: Sock,
}

impl DnsState {
    fn next_id(&mut self) -> u32 {
        let value = self.next;
        self.next = self.next.wrapping_add(1).max(1);
        value.max(1)
    }
}

/// The resolver socket implementation; borrows one [`Net`].
pub struct NetDns<'a, B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend = crate::NoUnix> {
    net: &'a Net<B, F, E, U>,
}

impl<'a, B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> NetDns<'a, B, F, E, U> {
    pub(crate) fn new(net: &'a Net<B, F, E, U>) -> Self {
        Self { net }
    }
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> Clock for NetDns<'_, B, F, E, U> {
    fn monotonic_ms(&self) -> u64 {
        self.net.env.monotonic_ms()
    }
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> Entropy for NetDns<'_, B, F, E, U> {
    fn fill(&self, buf: &mut [u8]) -> bool {
        self.net.env.entropy(buf)
    }
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> DnsIo for NetDns<'_, B, F, E, U> {
    fn open_udp(&self, family: Family) -> Result<UdpSock, i32> {
        if family == Family::V6 {
            return Err(i32::from(Errno::EAFNOSUPPORT.0));
        }
        let caps = self.net.backend.caps();
        let mut state = self.net.dns.borrow_mut();
        let id = state.next_id();
        if caps.bare_open {
            let mut buffer = [0u8; 16];
            let path = bare_path(SOCK_DGRAM, &mut buffer).map_err(errno)?;
            let sock = self.net.backend.open(path).map_err(errno)?;
            self.net
                .backend
                .call(
                    sock,
                    Call::Bind(Endpoint {
                        ip: [0, 0, 0, 0],
                        port: 0,
                    }),
                )
                .map_err(errno)?;
            state.udp.insert(
                id,
                UdpGroup {
                    members: Vec::new(),
                    bare: Some(sock),
                },
            );
        } else {
            state.udp.insert(
                id,
                UdpGroup {
                    members: Vec::new(),
                    bare: None,
                },
            );
        }
        Ok(UdpSock(id))
    }

    fn send_to(&self, s: UdpSock, buf: &[u8], to: &SockAddr) -> Result<usize, i32> {
        let endpoint = endpoint_of(to).ok_or(i32::from(Errno::EAFNOSUPPORT.0))?;
        let mut state = self.net.dns.borrow_mut();
        let group = state.udp.get_mut(&s.0).ok_or(i32::from(Errno::EBADF.0))?;
        if let Some(sock) = group.bare {
            return self
                .net
                .backend
                .write(sock, buf, Some(endpoint))
                .map_err(errno);
        }
        if let Some((_, sock)) = group.members.iter().find(|(peer, _)| *peer == endpoint) {
            return self.net.backend.write(*sock, buf, None).map_err(errno);
        }
        let mut buffer = [0u8; 64];
        let path = socket_path(SOCK_DGRAM, endpoint, &mut buffer).map_err(errno)?;
        let sock = self.net.backend.open(path).map_err(errno)?;
        group.members.push((endpoint, sock));
        self.net.backend.write(sock, buf, None).map_err(errno)
    }

    fn recv_from(&self, s: UdpSock, buf: &mut [u8]) -> Result<Recv, i32> {
        let mut state = self.net.dns.borrow_mut();
        let group = state.udp.get_mut(&s.0).ok_or(i32::from(Errno::EBADF.0))?;
        if let Some(sock) = group.bare {
            match self.net.backend.read(sock, buf) {
                Ok(done) => {
                    let from = done
                        .from
                        .map(|endpoint| SockAddr::new(IpAddr::V4(endpoint.ip), endpoint.port))
                        .unwrap_or_else(|| SockAddr::new(IpAddr::V4([0, 0, 0, 0]), 0));
                    return Ok(Recv {
                        len: done.len,
                        from,
                        truncated: done.truncated,
                    });
                }
                Err(Errno::EAGAIN) => return Err(i32::from(Errno::EAGAIN.0)),
                Err(error) => return Err(errno(error)),
            }
        }
        for (peer, sock) in group.members.iter() {
            match self.net.backend.read(*sock, buf) {
                Ok(done) => {
                    let source = done
                        .from
                        .map(|endpoint| SockAddr::new(IpAddr::V4(endpoint.ip), endpoint.port))
                        .unwrap_or_else(|| SockAddr::new(IpAddr::V4(peer.ip), peer.port));
                    return Ok(Recv {
                        len: done.len,
                        from: source,
                        truncated: done.truncated,
                    });
                }
                Err(Errno::EAGAIN) => {}
                Err(error) => return Err(errno(error)),
            }
        }
        Err(i32::from(Errno::EAGAIN.0))
    }

    fn open_tcp(&self, to: &SockAddr) -> Result<TcpSock, i32> {
        let endpoint = endpoint_of(to).ok_or(i32::from(Errno::EAFNOSUPPORT.0))?;
        let mut buffer = [0u8; 64];
        let path = socket_path(SOCK_STREAM, endpoint, &mut buffer).map_err(errno)?;
        let sock = self.net.backend.open(path).map_err(errno)?;
        let mut state = self.net.dns.borrow_mut();
        let id = state.next_id();
        state.tcp.insert(id, TcpEntry { sock });
        Ok(TcpSock(id))
    }

    fn tcp_send(&self, s: TcpSock, buf: &[u8]) -> Result<usize, i32> {
        let state = self.net.dns.borrow();
        let entry = state.tcp.get(&s.0).ok_or(i32::from(Errno::EBADF.0))?;
        self.net.backend.write(entry.sock, buf, None).map_err(errno)
    }

    fn tcp_recv(&self, s: TcpSock, buf: &mut [u8]) -> Result<usize, i32> {
        let state = self.net.dns.borrow();
        let entry = state.tcp.get(&s.0).ok_or(i32::from(Errno::EBADF.0))?;
        match self.net.backend.read(entry.sock, buf) {
            Ok(done) => Ok(done.len),
            Err(Errno::EAGAIN) => Err(i32::from(Errno::EAGAIN.0)),
            Err(error) => Err(errno(error)),
        }
    }

    fn wait(&self, interests: &mut [Interest], timeout_ms: u64) -> Result<(), i32> {
        let mut items: Vec<(usize, WaitItem)> = Vec::new();
        {
            let state = self.net.dns.borrow();
            for (index, interest) in interests.iter().enumerate() {
                match interest.kind {
                    InterestKind::Udp(sock) => {
                        if let Some(group) = state.udp.get(&sock.0) {
                            if let Some(bare) = group.bare {
                                items.push((
                                    index,
                                    WaitItem {
                                        sock: bare,
                                        want_read: !interest.write,
                                        want_write: interest.write,
                                        ..WaitItem::default()
                                    },
                                ));
                            }
                            for (_, member) in group.members.iter() {
                                items.push((
                                    index,
                                    WaitItem {
                                        sock: *member,
                                        want_read: !interest.write,
                                        want_write: interest.write,
                                        ..WaitItem::default()
                                    },
                                ));
                            }
                        }
                    }
                    InterestKind::Tcp(sock) => {
                        if let Some(entry) = state.tcp.get(&sock.0) {
                            items.push((
                                index,
                                WaitItem {
                                    sock: entry.sock,
                                    want_read: !interest.write,
                                    want_write: interest.write,
                                    ..WaitItem::default()
                                },
                            ));
                        }
                    }
                }
            }
        }
        let mut wait_items: Vec<WaitItem> = items.iter().map(|(_, item)| *item).collect();
        let ready = match self.net.backend.wait(&mut wait_items, Some(timeout_ms)) {
            Ok(ready) => ready,
            Err(Errno::EAGAIN) => 0,
            Err(error) => return Err(errno(error)),
        };
        if ready > 0 {
            for (slot, (index, _)) in items.iter().enumerate() {
                if let Some(item) = wait_items.get(slot)
                    && let Some(interest) = interests.get_mut(*index)
                {
                    if item.got_read {
                        interest.ready_r = true;
                    }
                    if item.got_write {
                        interest.ready_w = true;
                    }
                }
            }
        }
        Ok(())
    }

    fn close_udp(&self, s: UdpSock) {
        let mut state = self.net.dns.borrow_mut();
        if let Some(group) = state.udp.remove(&s.0) {
            if let Some(sock) = group.bare {
                self.net.backend.close(sock);
            }
            for (_, sock) in group.members {
                self.net.backend.close(sock);
            }
        }
    }

    fn close_tcp(&self, s: TcpSock) {
        let mut state = self.net.dns.borrow_mut();
        if let Some(entry) = state.tcp.remove(&s.0) {
            self.net.backend.close(entry.sock);
        }
    }
}

fn endpoint_of(addr: &SockAddr) -> Option<Endpoint> {
    match addr.ip {
        IpAddr::V4(ip) => Some(Endpoint {
            ip,
            port: addr.port,
        }),
        IpAddr::V6(_) => None,
    }
}

fn errno(error: Errno) -> i32 {
    i32::from(error.0)
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> Net<B, F, E, U> {
    /// The resolver socket seam.
    #[must_use]
    pub fn dns_io(&self) -> NetDns<'_, B, F, E, U> {
        NetDns::new(self)
    }
}
