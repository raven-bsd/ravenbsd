// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `__res_msend_rc`: one UDP socket, all servers in parallel, TC fallback to TCP.
//!
//! One fresh UDP socket per lookup (A-net-2), a total timeout rather than a per-server one
//! (T-12-18), reply ID and question verification (D-12-4) and a resumable TCP state (A-net-12).

use alloc::vec::Vec;

use raven_libc_abi::errno::{EAFNOSUPPORT, EAGAIN, EINVAL};

use crate::conf::{ConfExt, ResolvConf};
use crate::consts::{ABUF_SIZE, DNS_PORT, ENGINE_MAX_STEPS, MAXNS, TCP_MSG_MAX};
use crate::msg::question_matches;
use crate::traits::Env;
use crate::types::{Family, Interest, IpAddr, SockAddr, TcpSock, UdpSock};

struct TcpState {
    sock: TcpSock,
    frame: Vec<u8>,
    qpos: usize,
    buf: Vec<u8>,
    apos: usize,
    query: usize,
}

struct Sockets<'a> {
    io: &'a dyn crate::traits::DnsIo,
    udp: Option<UdpSock>,
    tcp: Vec<Option<TcpSock>>,
}

impl Drop for Sockets<'_> {
    fn drop(&mut self) {
        if let Some(sock) = self.udp.take() {
            self.io.close_udp(sock);
        }
        for slot in self.tcp.iter_mut() {
            if let Some(sock) = slot.take() {
                self.io.close_tcp(sock);
            }
        }
    }
}

fn normalize(addr: &SockAddr, family: Family) -> SockAddr {
    match family {
        Family::V6 => {
            let ip = match addr.ip {
                IpAddr::V4(_) => addr.ip.to_v4_mapped(),
                IpAddr::V6(_) => addr.ip,
            };
            SockAddr::with_scope(ip, addr.port, addr.scope)
        }
        Family::V4 => *addr,
    }
}

fn same_source(a: &SockAddr, b: &SockAddr, family: Family) -> bool {
    let left = normalize(a, family);
    let right = normalize(b, family);
    left.ip == right.ip && left.port == right.port && left.scope == right.scope
}

fn choose_family(conf: &ResolvConf, env: &Env) -> Result<(Family, Vec<SockAddr>), crate::Errno> {
    let mut has_v4 = false;
    let mut has_v6 = false;
    for index in 0..conf.nns.min(MAXNS) {
        if let Some(Some(server)) = conf.ns.get(index) {
            match server.ip {
                IpAddr::V4(_) => has_v4 = true,
                IpAddr::V6(_) => has_v6 = true,
            }
        }
    }
    if !has_v4 && !has_v6 {
        return Err(EINVAL);
    }
    let mut family = if has_v6 { Family::V6 } else { Family::V4 };
    if family == Family::V6 && !env.probe.family_usable(Family::V6) {
        if has_v4 {
            family = Family::V4;
        } else {
            return Err(EAFNOSUPPORT);
        }
    }
    let mut servers: Vec<SockAddr> = Vec::new();
    for index in 0..conf.nns.min(MAXNS) {
        let Some(Some(server)) = conf.ns.get(index) else {
            continue;
        };
        if family == Family::V4 && server.ip.family() == Family::V6 {
            continue;
        }
        let port = if server.port == 0 {
            DNS_PORT
        } else {
            server.port
        };
        servers.push(SockAddr::with_scope(
            match family {
                Family::V6 => match server.ip {
                    IpAddr::V4(_) => server.ip.to_v4_mapped(),
                    IpAddr::V6(_) => server.ip,
                },
                Family::V4 => server.ip,
            },
            port,
            server.scope,
        ));
    }
    if servers.is_empty() {
        return Err(EAFNOSUPPORT);
    }
    Ok((family, servers))
}

fn tcp_accept(
    state: &TcpState,
    answers: &mut [&mut [u8]],
    alens: &mut [usize],
    queries: &[&[u8]],
) -> bool {
    if state.apos < 2 {
        return false;
    }
    let alen = (usize::from(state.buf.first().copied().unwrap_or(0)) << 8)
        | usize::from(state.buf.get(1).copied().unwrap_or(0));
    if alen < 13 || state.apos < alen.saturating_add(2) {
        return false;
    }
    let Some(packet) = state.buf.get(2..2usize.saturating_add(alen)) else {
        return false;
    };
    let rcode = crate::u8_at(packet, 3) & 15;
    // 5 (REFUSED) is kept for the EAI_FAIL mapping required by ticket 7.2, as on the UDP path.
    if rcode != 0 && rcode != 3 && rcode != 5 {
        return false;
    }
    let Some(query) = queries.get(state.query).copied() else {
        return false;
    };
    if packet.get(..2) != query.get(..2) || !question_matches(query, packet) {
        return false;
    }
    let Some(slot) = answers.get_mut(state.query) else {
        return false;
    };
    let len = alen.min(slot.len());
    if let (Some(target), Some(source)) = (slot.get_mut(..len), packet.get(..len)) {
        target.copy_from_slice(source);
    }
    if let Some(length) = alens.get_mut(state.query) {
        *length = len;
    }
    true
}

/// The `res_msend` engine.
///
/// Fills `alens[i]` with the accepted answer length (0 when unanswered) and returns an error
/// only for a system-level failure. `answers[i]` must be at least the query size.
pub fn msend(
    env: &Env,
    conf: &ResolvConf,
    ext: &ConfExt,
    queries: &[&[u8]],
    answers: &mut [&mut [u8]],
    alens: &mut [usize],
) -> Result<(), crate::Errno> {
    let count = queries.len();
    if answers.len() < count || alens.len() < count {
        return Err(EINVAL);
    }
    if conf.attempts == 0 {
        return Err(EINVAL);
    }
    if count == 0 {
        return Ok(());
    }
    for length in alens.iter_mut() {
        *length = 0;
    }
    let (family, servers) = choose_family(conf, env)?;
    let mut sockets = Sockets {
        io: env.io,
        udp: None,
        tcp: alloc::vec![None; count],
    };
    let udp = env.io.open_udp(family)?;
    sockets.udp = Some(udp);

    let timeout = u64::from(conf.timeout).saturating_mul(1000);
    let attempts = u64::from(conf.attempts.max(1));
    let retry = timeout.checked_div(attempts).unwrap_or(0).max(1);
    let start = env.clock.monotonic_ms();
    let deadline = start.saturating_add(timeout);
    let mut next_send = start;
    let mut servfail_retry: i32 = i32::try_from(count.saturating_mul(2)).unwrap_or(i32::MAX);
    let mut states: Vec<Option<TcpState>> = Vec::new();
    for _ in 0..count {
        states.push(None);
    }
    let mut recv_buf = alloc::vec![0u8; ABUF_SIZE];
    let mut steps = 0usize;

    loop {
        let now = env.clock.monotonic_ms();
        if now >= deadline {
            break;
        }
        steps = steps.saturating_add(1);
        if steps > ENGINE_MAX_STEPS {
            return Err(EAGAIN);
        }
        if now >= next_send {
            for (index, query) in queries.iter().enumerate() {
                if alens.get(index).copied().unwrap_or(0) != 0
                    || states.get(index).is_some_and(Option::is_some)
                {
                    continue;
                }
                if ext.no_aaaa && query_type(query) == Some(28) {
                    continue;
                }
                for server in servers.iter() {
                    let _ = env.io.send_to(udp, query, server);
                }
            }
            next_send = now.saturating_add(retry);
            servfail_retry = i32::try_from(count.saturating_mul(2)).unwrap_or(i32::MAX);
        }

        let wake = next_send.min(deadline);
        let wait_ms = wake.saturating_sub(now).max(1);
        let mut interests: Vec<Interest> = Vec::new();
        interests.push(Interest::udp(udp));
        for (index, state) in states.iter().enumerate() {
            if let Some(value) = state {
                let write = value.qpos < value.frame.len();
                interests.push(Interest::tcp(value.sock, index, write));
            }
        }
        env.io.wait(&mut interests, wait_ms)?;

        let udp_ready = interests.first().is_some_and(|interest| interest.ready_r);
        if udp_ready {
            loop {
                match env.io.recv_from(udp, &mut recv_buf) {
                    Ok(received) => {
                        let len = received.len.min(recv_buf.len());
                        let Some(datagram) = recv_buf.get(..len) else {
                            continue;
                        };
                        if len < 4 {
                            continue;
                        }
                        let Some(server) = servers
                            .iter()
                            .find(|server| same_source(&received.from, server, family))
                            .copied()
                        else {
                            continue;
                        };
                        let mut matched: Option<usize> = None;
                        for (index, query) in queries.iter().enumerate() {
                            if alens.get(index).copied().unwrap_or(0) != 0
                                || states.get(index).is_some_and(Option::is_some)
                            {
                                continue;
                            }
                            if query.get(..2) == datagram.get(..2) {
                                matched = Some(index);
                                break;
                            }
                        }
                        let Some(index) = matched else {
                            continue;
                        };
                        let Some(query) = queries.get(index).copied() else {
                            continue;
                        };
                        if !question_matches(query, datagram) {
                            continue;
                        }
                        let rcode = crate::u8_at(datagram, 3) & 15;
                        match rcode {
                            // 0 and 3 are the donor's accepted answers. 5 (REFUSED) is stored
                            // so `lookup_name` can report EAI_FAIL, which ticket 7.2 requires;
                            // the donor engine would ignore it and eventually report EAI_AGAIN.
                            0 | 3 | 5 => {}
                            2 => {
                                if servfail_retry > 0 {
                                    servfail_retry = servfail_retry.saturating_sub(1);
                                    let _ = env.io.send_to(udp, query, &server);
                                }
                                continue;
                            }
                            _ => continue,
                        }
                        let truncated = received.truncated
                            || crate::u8_at(datagram, 2) & 2 != 0
                            || len > answers.get(index).map_or(0, |slot| slot.len());
                        if truncated {
                            if let Ok(sock) = env.io.open_tcp(&received.from) {
                                let mut frame = Vec::with_capacity(len.saturating_add(2));
                                if let Ok(prefix) = u16::try_from(query.len()) {
                                    frame.push(u8::try_from(prefix >> 8).unwrap_or(0));
                                    frame.push(u8::try_from(prefix & 0xff).unwrap_or(0));
                                }
                                frame.extend_from_slice(query);
                                let tcp_buf = alloc::vec![0u8; TCP_MSG_MAX.saturating_add(2)];
                                if let Some(slot) = sockets.tcp.get_mut(index) {
                                    *slot = Some(sock);
                                }
                                if let Some(state) = states.get_mut(index) {
                                    *state = Some(TcpState {
                                        sock,
                                        frame,
                                        qpos: 0,
                                        buf: tcp_buf,
                                        apos: 0,
                                        query: index,
                                    });
                                }
                            }
                            continue;
                        }
                        let Some(slot) = answers.get_mut(index) else {
                            continue;
                        };
                        let copy_len = len.min(slot.len());
                        if let (Some(target), Some(source)) =
                            (slot.get_mut(..copy_len), datagram.get(..copy_len))
                        {
                            target.copy_from_slice(source);
                        }
                        if let Some(length) = alens.get_mut(index) {
                            *length = copy_len;
                        }
                    }
                    Err(error) if error == EAGAIN => break,
                    Err(error) => return Err(error),
                }
            }
        }

        for interest in interests.iter().skip(1) {
            let index = interest.query;
            if index >= count {
                continue;
            }
            let Some(Some(state)) = states.get_mut(index) else {
                continue;
            };
            if interest.write {
                if !interest.ready_w {
                    continue;
                }
                let start = state.qpos;
                let Some(remaining) = state.frame.get(start..) else {
                    continue;
                };
                match env.io.tcp_send(state.sock, remaining) {
                    Ok(0) | Err(_) => {
                        let sock = state.sock;
                        if let Some(slot) = sockets.tcp.get_mut(index) {
                            *slot = None;
                        }
                        env.io.close_tcp(sock);
                        if let Some(slot) = states.get_mut(index) {
                            *slot = None;
                        }
                    }
                    Ok(written) => {
                        state.qpos = state.qpos.saturating_add(written);
                    }
                }
            } else {
                if !interest.ready_r {
                    continue;
                }
                let start = state.apos;
                let Some(target) = state.buf.get_mut(start..) else {
                    continue;
                };
                match env.io.tcp_recv(state.sock, target) {
                    Ok(0) | Err(_) => {
                        let sock = state.sock;
                        if let Some(slot) = sockets.tcp.get_mut(index) {
                            *slot = None;
                        }
                        env.io.close_tcp(sock);
                        if let Some(slot) = states.get_mut(index) {
                            *slot = None;
                        }
                    }
                    Ok(read) => {
                        state.apos = state.apos.saturating_add(read);
                        if tcp_accept(state, answers, alens, queries) {
                            let sock = state.sock;
                            if let Some(slot) = sockets.tcp.get_mut(index) {
                                *slot = None;
                            }
                            env.io.close_tcp(sock);
                            if let Some(slot) = states.get_mut(index) {
                                *slot = None;
                            }
                        }
                    }
                }
            }
        }

        if alens.iter().all(|length| *length > 0) {
            break;
        }
    }
    Ok(())
}

/// The qtype field of a query, when the packet is well formed.
#[must_use]
pub fn query_type(query: &[u8]) -> Option<u16> {
    if query.len() < 12 {
        return None;
    }
    let mut position = 12usize;
    loop {
        let byte = query.get(position).copied()?;
        if byte == 0 {
            position = position.checked_add(1)?;
            break;
        }
        if byte & 0xc0 == 0xc0 {
            position = position.checked_add(2)?;
            break;
        }
        position = position.checked_add(1)?.checked_add(usize::from(byte))?;
    }
    crate::be16_at(query, position)
}

#[cfg(test)]
#[path = "engine_ut.rs"]
mod tests;
