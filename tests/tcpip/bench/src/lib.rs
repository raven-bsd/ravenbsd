// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Benchmark scenarios for the native `raven-tcpip` stack.

#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

use raven_tcpip::{
    Instant as SimInstant, IpAddr, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};

/// One deterministic or wall-clock measurement.
#[derive(Clone, Debug, Default)]
pub struct Run {
    /// Payload bytes delivered and verified.
    pub bytes: usize,
    /// Virtual milliseconds from connect to completion.
    pub virtual_ms: i64,
    /// TCP segments observed/estimated, when available.
    pub segments: Option<u64>,
    /// Retransmitted payload bytes, when available.
    pub retrans_bytes: Option<u64>,
    /// Wall-clock duration.
    pub wall: Duration,
}

impl Run {
    /// Virtual goodput in MiB/s.
    #[must_use]
    pub fn goodput_mib_s(&self) -> f64 {
        if self.virtual_ms <= 0 {
            return 0.0;
        }
        (self.bytes as f64 / (1024.0 * 1024.0)) / (self.virtual_ms as f64 / 1000.0)
    }
}

fn pattern(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

fn hash(bytes: &[u8]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn raven_pair(
    rtt_ms: u64,
    loss_percent: f32,
    reorder_percent: f32,
) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(
        LinkConfig {
            rtt_ms,
            loss_percent,
            reorder_percent,
            ..LinkConfig::default()
        },
        131,
    );
    let link = a.link();
    let mut a = Stack::new(a, StackConfig::default(), 1, SimInstant::ZERO);
    let mut b = Stack::new(b, StackConfig::default(), 2, SimInstant::ZERO);
    for (stack, address) in [(&mut a, A), (&mut b, B)] {
        stack
            .set_ipv4(Ipv4Config {
                addr: Ipv4Addr::new(address),
                prefix_len: 24,
                gateway: None,
            })
            .expect("ipv4");
    }
    a.test_insert_neighbor(Ipv4Addr::new(B), BMAC, SimInstant::ZERO, 600_000);
    b.test_insert_neighbor(Ipv4Addr::new(A), AMAC, SimInstant::ZERO, 600_000);
    (link, a, b)
}

/// Transfer 1 MiB (or `bytes`) over `raven-tcpip`, with a deterministic virtual clock.
pub fn raven_transfer(bytes: usize, rtt_ms: u64, loss_percent: f32, reorder_percent: f32) -> Run {
    let (link, mut a, mut b) = raven_pair(rtt_ms, loss_percent, reorder_percent);
    let start = Instant::now();
    let mut time = 0i64;
    b.tcp_listen(80, 8).expect("listen");
    let client = a
        .tcp_connect((IpAddr::V4(Ipv4Addr::new(B)), 80), Some(40_000))
        .expect("connect");
    let mut server = None;
    for _ in 0..20_000 {
        if server.is_none()
            && let Ok(Some(socket)) = b.tcp_accept(SocketId(1))
        {
            server = Some(socket);
        }
        if server.is_some() {
            break;
        }
        time += 1;
        let now = SimInstant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    let server = server.expect("accept");
    let connect_ms = time;
    let payload = pattern(bytes);
    let mut sent = 0usize;
    let mut received = Vec::new();
    let mut buffer = vec![0u8; 65_536];
    for _ in 0..2_000_000 {
        if sent < payload.len()
            && let Ok(count) = a.send(client, &payload[sent..])
        {
            sent += count;
        }
        loop {
            match b.recv(server, &mut buffer) {
                Ok(0) => break,
                Ok(count) => received.extend_from_slice(&buffer[..count]),
                Err(_) => break,
            }
        }
        if received.len() >= payload.len() {
            break;
        }
        time += 1;
        let now = SimInstant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    assert_eq!(
        received.len(),
        payload.len(),
        "short transfer: received {} of {}",
        received.len(),
        payload.len()
    );
    assert_eq!(hash(&received), hash(&payload), "payload corruption");
    let wall = start.elapsed();
    Run {
        bytes,
        virtual_ms: time.saturating_sub(connect_ms),
        segments: Some(u64::try_from(link.trace().entries().len()).unwrap_or(u64::MAX)),
        retrans_bytes: Some(a.counters().retrans_bytes),
        wall,
    }
}

/// Virtual time to first byte after connect.
pub fn raven_ttfb(rtt_ms: u64) -> Run {
    let (_link, mut a, mut b) = raven_pair(rtt_ms, 0.0, 0.0);
    let start = Instant::now();
    b.tcp_listen(80, 8).expect("listen");
    let client = a
        .tcp_connect((IpAddr::V4(Ipv4Addr::new(B)), 80), Some(40_000))
        .expect("connect");
    let mut time = 0i64;
    let mut server = None;
    for _ in 0..20_000 {
        if let Ok(Some(socket)) = b.tcp_accept(SocketId(1)) {
            server = Some(socket);
            break;
        }
        time += 1;
        let now = SimInstant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    let server = server.expect("accept");
    a.send(client, b"x").expect("send");
    let mut buffer = [0u8; 8];
    for _ in 0..20_000 {
        let now = SimInstant::from_millis(time);
        a.poll(now);
        b.poll(now);
        if let Ok(count) = b.recv(server, &mut buffer)
            && count > 0
        {
            break;
        }
        time += 1;
    }
    Run {
        bytes: 1,
        virtual_ms: time,
        segments: None,
        retrans_bytes: None,
        wall: start.elapsed(),
    }
}

/// Resident memory in KiB from `/proc/self/statm`.
pub fn rss_kib() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = text.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages.saturating_mul(4))
}
