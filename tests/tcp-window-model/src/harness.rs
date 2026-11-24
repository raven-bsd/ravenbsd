// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic real-stack harness for the receive-window model.

use raven_tcpip::{Instant, IpAddr, Ipv4Addr, Ipv4Config, Stack, StackConfig};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Header, Ipv4Packet};
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpPacket};

/// Local test address.
pub const A: [u8; 4] = [192, 0, 2, 1];
/// Peer test address.
pub const B: [u8; 4] = [192, 0, 2, 2];
/// Local MAC.
pub const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
/// Peer MAC.
pub const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

/// A host address.
#[must_use]
pub fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

fn endpoint_stack(
    endpoint: Endpoint,
    address_value: [u8; 4],
    seed: u64,
    config: StackConfig,
) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, config, seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(address_value),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack.test_set_trace(true);
    stack
}

/// Two linked stacks with the requested receive-buffer capacity.
#[must_use]
pub fn pair_with(rx_queue_bytes: usize) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let config = StackConfig {
        rx_queue_bytes,
        ..StackConfig::default()
    };
    let (a, b) = Link::new(LinkConfig::default(), 310);
    let link = a.link();
    let mut a = endpoint_stack(a, A, 1, config);
    let mut b = endpoint_stack(b, B, 2, config);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

/// Complete a three-way handshake on port 80.
#[must_use]
pub fn handshake(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
) -> (raven_tcpip::SocketId, raven_tcpip::SocketId) {
    b.tcp_listen(80, 4).expect("listen");
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    pump(a, b, 0, 10, 25);
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("connection");
    (socket, accepted)
}

/// Poll both stacks `count` times, advancing by `step_ms`.
pub fn pump(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    start_ms: i64,
    step_ms: i64,
    count: usize,
) {
    let mut now = start_ms;
    for _ in 0..count {
        let instant = Instant::from_millis(now);
        a.poll(instant);
        b.poll(instant);
        now += step_ms;
    }
}

/// Drain the receive queue in `chunk`-sized reads.
#[must_use]
pub fn drain(b: &mut Stack<Endpoint>, socket: raven_tcpip::SocketId, chunk: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buffer = vec![0u8; chunk.max(1)];
    loop {
        match b.recv(socket, &mut buffer) {
            Ok(0) => break,
            Ok(count) => out.extend_from_slice(&buffer[..count]),
            Err(raven_abi::errno::Errno::EAGAIN) => break,
            Err(_) => break,
        }
    }
    out
}

/// A duplicate/window ACK frame from the peer.
pub fn ack_frame(seq: u32, ack: u32, window: u16) -> Vec<u8> {
    let header = TcpHeader {
        src_port: 80,
        dst_port: 40_000,
        sequence: seq,
        acknowledgement: ack,
        flags: TcpFlags::ACK,
        window,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(B), WireIpv4Addr::new(A));
    let mut segment = vec![0u8; 20];
    {
        let builder = TcpBuilder::new(&mut segment, context, &header, &[], 0).expect("tcp");
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 310,
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
    frame
}

/// Parse TCP frames on a link after `start` as `(flags, sequence, acknowledgement, payload length)`.
#[must_use]
pub fn trace_link(link: &Link, start: usize) -> Vec<(TcpFlags, u32, u32, usize)> {
    link.trace()
        .entries()
        .iter()
        .skip(start)
        .filter_map(|entry| {
            let ethernet = EthernetFrame::parse(&entry.bytes).ok()?;
            if ethernet.ether_type() != EtherType::Ipv4 {
                return None;
            }
            let ipv4 = Ipv4Packet::parse(ethernet.payload()).ok()?;
            if ipv4.protocol() != 6 {
                return None;
            }
            let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
            let tcp = TcpPacket::parse(ipv4.payload(), context).ok()?;
            Some((
                tcp.flags(),
                tcp.sequence(),
                tcp.acknowledgement(),
                tcp.payload().len(),
            ))
        })
        .collect()
}
