// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side TCP output stress and data-integrity matrix.
//!
//! The crate drives the merged `raven-tcpip` stack over the deterministic `raven-tcpip-sim` link.
//! Every scenario hashes the delivered stream; the expected behaviour is the FreeBSD
//! `tcp_output.c`/`tcp_timer.c` path ported by K265. This crate is test tooling only: it makes no
//! product change.

#![forbid(unsafe_code)]

use raven_abi::errno::Errno;
use raven_tcpip::{Instant, IpAddr, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};

/// Client hardware address (matches the simulator's endpoint A).
pub const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x0a];
/// Server hardware address (matches the simulator's endpoint B).
pub const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x0b];
/// Client address.
pub const A: [u8; 4] = [10, 0, 0, 1];
/// Server address.
pub const B: [u8; 4] = [10, 0, 0, 2];

/// FNV-1a 64-bit stream hash (stable across runs and hosts).
#[must_use]
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Deterministic stream payload.
#[must_use]
pub fn payload(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index * 31 + 7) as u8).collect()
}

fn endpoint_stack(
    endpoint: Endpoint,
    octets: [u8; 4],
    seed: u64,
    config: StackConfig,
    peer: [u8; 4],
    peer_mac: [u8; 6],
) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, config, seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new(octets),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack.test_insert_neighbor(Ipv4Addr::new(peer), peer_mac, Instant::ZERO, 600_000);
    stack
}

/// A connected A/B pair over the simulated link.
#[must_use]
pub fn pair(
    config: StackConfig,
    link_config: LinkConfig,
    seed: u64,
) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (client_end, server_end) = Link::new(link_config, seed);
    let link = client_end.link();
    let client = endpoint_stack(client_end, A, seed, config, B, BMAC);
    let server = endpoint_stack(server_end, B, seed + 1, config, A, AMAC);
    (link, client, server)
}

/// Poll both stacks from `from` to `to` inclusive, one virtual millisecond per step.
pub fn pump(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, from: i64, to: i64) {
    let mut time = from;
    while time <= to {
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        time += 1;
    }
}

/// Establish a client/server TCP connection and return both socket ids.
pub fn handshake(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>) -> (SocketId, SocketId) {
    b.tcp_listen(80, 8).expect("listen");
    let connected = a
        .tcp_connect((IpAddr::V4(Ipv4Addr::new(B)), 80), Some(40_000))
        .expect("connect");
    pump(a, b, 0, 200);
    let accepted = b
        .tcp_accept(SocketId(1))
        .expect("accept")
        .expect("connection");
    (connected, accepted)
}

fn drain(b: &mut Stack<Endpoint>, socket: SocketId, out: &mut Vec<u8>, buffer: &mut [u8]) {
    loop {
        match b.recv(socket, buffer) {
            Ok(0) => break,
            Ok(count) => out.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => break,
            Err(error) => panic!("recv: {error:?}"),
        }
    }
}

fn stall(a: &Stack<Endpoint>, socket: SocketId, sent: usize, received: usize) -> ! {
    let info = a.socket_info(socket).expect("socket info");
    panic!(
        "transfer stalled: sent={sent} received={received} mss={} cwnd={} state={:?}",
        info.mss, info.cwnd, info.state
    )
}

/// Send `data` over an established connection and return everything the peer received.
///
/// The caller advances `now`; the transfer has a hard virtual-time cap of 120 s.
pub fn send_stream(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    send_socket: SocketId,
    recv_socket: SocketId,
    data: &[u8],
    now: &mut i64,
) -> Vec<u8> {
    let start = *now;
    let mut sent = 0usize;
    let mut received = Vec::new();
    let mut buffer = vec![0u8; 65_536];
    while sent < data.len() || received.len() < data.len() {
        if sent < data.len() {
            match a.send(send_socket, &data[sent..]) {
                Ok(count) => sent += count,
                Err(Errno::EAGAIN) => {}
                Err(error) => panic!("send: {error:?}"),
            }
        }
        *now += 1;
        a.poll(Instant::from_millis(*now));
        b.poll(Instant::from_millis(*now));
        drain(b, recv_socket, &mut received, &mut buffer);
        if *now - start > 120_000 {
            stall(a, send_socket, sent, received.len());
        }
    }
    received
}

/// Send a sequence of application records; each `send` is a separate write.
pub fn send_records(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    send_socket: SocketId,
    recv_socket: SocketId,
    records: &[Vec<u8>],
    now: &mut i64,
) -> Vec<u8> {
    let start = *now;
    let expected: usize = records.iter().map(Vec::len).sum();
    let mut received = Vec::new();
    let mut buffer = vec![0u8; 65_536];
    for record in records {
        let mut offset = 0usize;
        while offset < record.len() {
            match a.send(send_socket, &record[offset..]) {
                Ok(count) => offset += count,
                Err(Errno::EAGAIN) => {
                    *now += 1;
                    a.poll(Instant::from_millis(*now));
                    b.poll(Instant::from_millis(*now));
                    drain(b, recv_socket, &mut received, &mut buffer);
                }
                Err(error) => panic!("send: {error:?}"),
            }
        }
        *now += 1;
        a.poll(Instant::from_millis(*now));
        b.poll(Instant::from_millis(*now));
        drain(b, recv_socket, &mut received, &mut buffer);
        if *now - start > 120_000 {
            stall(a, send_socket, offset, received.len());
        }
    }
    while received.len() < expected {
        *now += 1;
        a.poll(Instant::from_millis(*now));
        b.poll(Instant::from_millis(*now));
        drain(b, recv_socket, &mut received, &mut buffer);
        if *now - start > 120_000 {
            stall(a, send_socket, expected, received.len());
        }
    }
    received
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
