// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic real-stack harness for the input model properties.

use raven_tcpip::{
    Device, Instant, IpAddr, Ipv4Addr, Ipv4Config, LoopbackDevice, Stack, StackConfig,
};
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

fn endpoint_stack(endpoint: Endpoint, address_value: [u8; 4], seed: u64) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, StackConfig::default(), seed, Instant::ZERO);
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

/// Two linked stacks.
#[must_use]
pub fn pair() -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(LinkConfig::default(), 306);
    let link = a.link();
    let mut a = endpoint_stack(a, A, 1);
    let mut b = endpoint_stack(b, B, 2);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

/// One loopback stack with the peer's neighbour entry installed.
#[must_use]
pub fn stack4() -> Stack<LoopbackDevice> {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        306,
        Instant::ZERO,
    );
    stack
        .set_ipv4(Ipv4Config {
            addr: address(A),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    stack.test_set_trace(true);
    stack
}

/// Poll both stacks at the supplied virtual times.
pub fn drive(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, times: &[i64]) {
    for time in times {
        let now = Instant::from_millis(*time);
        a.poll(now);
        b.poll(now);
    }
}

/// Complete a three-way handshake on port 80.
#[must_use]
pub fn handshake(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
) -> (raven_tcpip::SocketId, raven_tcpip::SocketId) {
    handshake_at(a, b, 0)
}

/// Complete a handshake with virtual time starting at `base_ms`.
#[must_use]
pub fn handshake_at(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    base_ms: i64,
) -> (raven_tcpip::SocketId, raven_tcpip::SocketId) {
    b.tcp_listen(80, 4).expect("listen");
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    drive(
        a,
        b,
        &[
            base_ms,
            base_ms + 50,
            base_ms + 100,
            base_ms + 150,
            base_ms + 200,
        ],
    );
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("connection");
    (socket, accepted)
}

/// Build one IPv4/TCP frame. `src`/`dst` select the direction and MAC addresses.
#[allow(clippy::too_many_arguments)] // Mirrors the wire header fields.
pub fn frame(
    src: [u8; 4],
    dst: [u8; 4],
    source_port: u16,
    destination_port: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    window: u16,
    options: &[u8],
    payload: &[u8],
) -> Vec<u8> {
    let header = TcpHeader {
        src_port: source_port,
        dst_port: destination_port,
        sequence: seq,
        acknowledgement: ack,
        flags,
        window,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(src), WireIpv4Addr::new(dst));
    let padded = (options.len() + 3) & !3;
    let mut segment = vec![0u8; 20 + padded + payload.len()];
    {
        let mut builder =
            TcpBuilder::new(&mut segment, context, &header, options, payload.len()).expect("tcp");
        builder.payload_mut().copy_from_slice(payload);
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 306,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(src),
        dst: WireIpv4Addr::new(dst),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + segment.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &ip_header, segment.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(&segment);
    builder.finish();
    let (source_mac, destination_mac) = if src == A { (AMAC, BMAC) } else { (BMAC, AMAC) };
    let mut frame = vec![0u8; 14 + packet.len()];
    let mut builder = EthernetBuilder::new(
        &mut frame,
        MacAddr::new(source_mac),
        MacAddr::new(destination_mac),
        EtherType::Ipv4,
        packet.len(),
    )
    .expect("ethernet");
    builder.payload_mut().copy_from_slice(&packet);
    builder.finish();
    frame
}

fn parse_trace(entries: &[Vec<u8>]) -> Vec<(TcpFlags, u32, u32)> {
    entries
        .iter()
        .filter_map(|entry| {
            let ethernet = EthernetFrame::parse(entry).ok()?;
            if ethernet.ether_type() != EtherType::Ipv4 {
                return None;
            }
            let ipv4 = Ipv4Packet::parse(ethernet.payload()).ok()?;
            if ipv4.protocol() != 6 {
                return None;
            }
            let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
            let tcp = TcpPacket::parse(ipv4.payload(), context).ok()?;
            Some((tcp.flags(), tcp.sequence(), tcp.acknowledgement()))
        })
        .collect()
}

/// Parse TCP frames emitted by a loopback stack.
#[must_use]
pub fn trace_stack<D: Device>(stack: &Stack<D>) -> Vec<(TcpFlags, u32, u32)> {
    let entries: Vec<Vec<u8>> = stack.test_trace().to_vec();
    parse_trace(&entries)
}

/// Parse TCP frames emitted on a link after `start`.
#[must_use]
pub fn trace_link(link: &Link, start: usize) -> Vec<(TcpFlags, u32, u32)> {
    let entries: Vec<Vec<u8>> = link
        .trace()
        .entries()
        .iter()
        .skip(start)
        .map(|entry| entry.bytes.clone())
        .collect();
    parse_trace(&entries)
}
