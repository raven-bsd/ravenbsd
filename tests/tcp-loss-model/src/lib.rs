// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only TCP loss/retransmit/reordering model (K317).
//!
//! The crate drives the public `raven-tcpip` stack over the deterministic sim link and compares
//! every wire data segment with an independent byte-offset reference oracle. The oracle checks
//! the sequence, payload bytes, coverage, retransmission/reordering shape and that no ACK covers
//! bytes that were never sent. The delivered stream is hashed. No product code is copied here.

#![forbid(unsafe_code)]

use core::cell::Cell;
use std::rc::Rc;

use raven_abi::errno::Errno;
use raven_tcpip::{
    Device, DeviceCaps, Instant, IpAddr, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig,
    TxError,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetFrame};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireIpv4Addr, Ipv4Packet};
use raven_tcpip_wire::tcp::TcpPacket;

/// Client address.
pub const A: [u8; 4] = [192, 0, 2, 1];
/// Server address.
pub const B: [u8; 4] = [192, 0, 2, 2];
/// Client MAC (matches the simulator's endpoint A).
pub const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x0a];
/// Server MAC (matches the simulator's endpoint B).
pub const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x0b];

/// FNV-1a 64-bit stream hash.
#[must_use]
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Deterministic payload for a seed.
#[must_use]
pub fn payload(seed: u64, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| {
            let value = (index as u64)
                .wrapping_mul(31)
                .wrapping_add(seed.wrapping_mul(7) + 11);
            (value % 251) as u8
        })
        .collect()
}

/// SplitMix64.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit.max(1)
    }
}

/// One randomized loss/reorder scenario.
#[derive(Clone, Debug)]
pub struct Scenario {
    /// Loss percentage.
    pub loss: f32,
    /// Duplicate percentage.
    pub duplicate: f32,
    /// Reorder percentage.
    pub reorder: f32,
    /// One-way RTT, milliseconds.
    pub rtt: u64,
    /// Original stream.
    pub payload: Vec<u8>,
}

impl Scenario {
    /// Build scenario `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let loss = rng.below(11) as f32;
        let duplicate = rng.below(6) as f32;
        let reorder = rng.below(11) as f32;
        let rtt = 5 + rng.below(6);
        let segments = 3 + rng.below(6) as usize;
        let segment_len = 128 + rng.below(385) as usize;
        let payload = payload(seed, segments * segment_len);
        Self {
            loss,
            duplicate,
            reorder,
            rtt,
            payload,
        }
    }
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
pub fn pump<A: Device, B: Device>(a: &mut Stack<A>, b: &mut Stack<B>, from: i64, to: i64) {
    let mut time = from;
    while time <= to {
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        time += 1;
    }
}

/// Establish a client/server TCP connection and return both socket ids.
pub fn handshake<A: Device, B: Device>(a: &mut Stack<A>, b: &mut Stack<B>) -> (SocketId, SocketId) {
    b.tcp_listen(80, 8).expect("listen");
    let connected = a
        .tcp_connect((IpAddr::V4(Ipv4Addr::new(B)), 80), Some(40_000))
        .expect("connect");
    pump(a, b, 0, 60);
    let accepted = b
        .tcp_accept(SocketId(1))
        .expect("accept")
        .expect("connection");
    (connected, accepted)
}

fn drain<B: Device>(b: &mut Stack<B>, socket: SocketId, out: &mut Vec<u8>, buffer: &mut [u8]) {
    loop {
        match b.recv(socket, buffer) {
            Ok(0) => break,
            Ok(count) => out.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => break,
            Err(error) => panic!("recv: {error:?}"),
        }
    }
}

/// Send `data` and return the delivered stream, with a hard virtual-time cap.
pub fn send_stream<A: Device, B: Device>(
    a: &mut Stack<A>,
    b: &mut Stack<B>,
    send_socket: SocketId,
    recv_socket: SocketId,
    data: &[u8],
    now: &mut i64,
    label: u64,
) -> Vec<u8> {
    let start = *now;
    let mut sent = 0usize;
    let mut received = Vec::new();
    let mut buffer = vec![0u8; 65_536];
    while received.len() < data.len() {
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
        if *now - start > 90_000 {
            let info = a.socket_info(send_socket).expect("info");
            panic!(
                "seed {label}: stalled at {} bytes, sent {sent}: mss={} cwnd={} state={:?}",
                received.len(),
                info.mss,
                info.cwnd,
                info.state
            );
        }
    }
    received
}

/// Oracle statistics for one scenario.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OracleStats {
    /// Data segments on the wire.
    pub segments: usize,
    /// Segments that overlapped an already covered byte.
    pub retransmits: usize,
    /// New segments that arrived behind the current high-water mark.
    pub out_of_order: usize,
    /// Highest acknowledged byte offset.
    pub max_ack: usize,
}

/// Independent byte-offset reference oracle over the wire trace.
///
/// Every data segment must carry exactly the original bytes at its sequence offset; the union
/// of the segment ranges must cover the stream; an ACK may never acknowledge bytes beyond the
/// highest offset actually sent at that point in the trace.
pub fn validate_trace(
    trace: &raven_tcpip_sim::Trace,
    stream: &[u8],
    first_seq: u32,
) -> Result<OracleStats, String> {
    let mut covered = vec![false; stream.len()];
    let mut stats = OracleStats::default();
    let mut max_end = 0usize;
    for entry in trace.entries() {
        let ethernet =
            EthernetFrame::parse(&entry.bytes).map_err(|e| format!("ethernet: {e:?}"))?;
        if ethernet.ether_type() != EtherType::Ipv4 {
            continue;
        }
        let ipv4 = Ipv4Packet::parse(ethernet.payload()).map_err(|e| format!("ipv4: {e:?}"))?;
        if ipv4.protocol() != 6 {
            continue;
        }
        let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
        let tcp = TcpPacket::parse(ipv4.payload(), context).map_err(|e| format!("tcp: {e:?}"))?;
        if ipv4.src() == WireIpv4Addr::new(A) && tcp.payload_len() > 0 {
            let offset = tcp.sequence().wrapping_sub(first_seq) as usize;
            let end = offset
                .checked_add(tcp.payload_len())
                .ok_or("offset overflow")?;
            if end > stream.len() {
                return Err(format!(
                    "segment {offset}..{end} beyond stream {}",
                    stream.len()
                ));
            }
            if tcp.payload() != &stream[offset..end] {
                return Err(format!("payload mismatch at offset {offset}"));
            }
            if covered[offset..end].iter().any(|byte| *byte) {
                stats.retransmits += 1;
            } else if offset < max_end {
                stats.out_of_order += 1;
            }
            for byte in &mut covered[offset..end] {
                *byte = true;
            }
            stats.segments += 1;
            max_end = max_end.max(end);
        } else if ipv4.src() == WireIpv4Addr::new(B) && tcp.flags().ack() {
            let ack = tcp.acknowledgement().wrapping_sub(first_seq) as usize;
            if ack > max_end {
                return Err(format!("ACK {ack} for unsent data (max sent {max_end})"));
            }
            stats.max_ack = stats.max_ack.max(ack);
        }
    }
    if covered.iter().any(|byte| !byte) {
        return Err("the wire segments do not cover the stream".to_string());
    }
    Ok(stats)
}

/// A client device that drops outgoing data frames at a configured rate after the handshake.
pub struct LossGate {
    inner: Endpoint,
    enabled: Rc<Cell<bool>>,
    loss_percent: f32,
    rng: Rc<Cell<u64>>,
}

impl LossGate {
    fn dropped(&self) -> bool {
        let mut state = self.rng.get();
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.rng.set(state);
        let sample = (state >> 33) % 10_000;
        sample < (f64::from(self.loss_percent) * 100.0) as u64
    }
}

impl Device for LossGate {
    fn capabilities(&self) -> DeviceCaps {
        self.inner.capabilities()
    }

    fn receive(&mut self, now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        self.inner.receive(now, consume)
    }

    fn transmit(
        &mut self,
        now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        if self.enabled.get() && self.dropped() {
            let mut data = vec![0u8; len];
            fill(&mut data);
            return Ok(());
        }
        self.inner.transmit(now, len, fill)
    }
}

/// A pair whose client data path can be switched to a loss rate after the handshake.
#[must_use]
pub fn pair_lossy(
    config: StackConfig,
    link_config: LinkConfig,
    seed: u64,
    loss_percent: f32,
) -> (Link, Stack<LossGate>, Stack<Endpoint>, Rc<Cell<bool>>) {
    let (client_end, server_end) = Link::new(link_config, seed);
    let link = client_end.link();
    let enabled = Rc::new(Cell::new(false));
    let rng = Rc::new(Cell::new(seed ^ 0x9e37_79b9_7f4a_7c15));
    let mut client = Stack::new(
        LossGate {
            inner: client_end,
            enabled: Rc::clone(&enabled),
            loss_percent,
            rng,
        },
        config,
        seed,
        Instant::ZERO,
    );
    client
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new(A),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    client.test_insert_neighbor(Ipv4Addr::new(B), BMAC, Instant::ZERO, 600_000);
    let server = endpoint_stack(server_end, B, seed + 1, config, A, AMAC);
    (link, client, server, enabled)
}

/// Inject a duplicate ACK from B to A into `a`.
pub fn inject_duplicate_ack(a: &mut Stack<Endpoint>, seq: u32, ack: u32, now: i64) {
    use raven_tcpip_wire::ethernet::{EthernetBuilder, MacAddr};
    use raven_tcpip_wire::ipv4::{Ipv4Builder, Ipv4Header};
    use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader};

    let tcp_header = TcpHeader {
        src_port: 80,
        dst_port: 40_000,
        sequence: seq,
        acknowledgement: ack,
        flags: TcpFlags::ACK,
        window: 65_535,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(B), WireIpv4Addr::new(A));
    let mut segment = vec![0u8; 20];
    {
        let builder = TcpBuilder::new(&mut segment, context, &tcp_header, &[], 0).expect("tcp");
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 11,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(B),
        dst: WireIpv4Addr::new(A),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + segment.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &ip_header, segment.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(&segment);
    builder.finish();
    let mut frame = vec![0u8; 14 + packet.len()];
    let mut builder = EthernetBuilder::new(
        &mut frame,
        MacAddr::new(BMAC),
        MacAddr::new(AMAC),
        EtherType::Ipv4,
        packet.len(),
    )
    .expect("ethernet");
    builder.payload_mut().copy_from_slice(&packet);
    builder.finish();
    a.test_inject_frame(&frame, Instant::from_millis(now));
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
