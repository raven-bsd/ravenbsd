// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K322: a CE-marked TCP segment must make the receiver echo ECE on its ACK.

#![cfg(feature = "testutil")]

mod common;

use raven_tcpip::{Instant, Ipv4Addr, Ipv4Config, Stack, StackConfig};
use raven_tcpip_sim::Endpoint;
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Header};
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn pair() -> (raven_tcpip_sim::Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = raven_tcpip_sim::Link::new(raven_tcpip_sim::LinkConfig::default(), 71);
    let link = a.link();
    let mut a = stack(a, A, 1);
    let mut b = stack(b, B, 2);
    a.test_insert_neighbor(Ipv4Addr::new(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(Ipv4Addr::new(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

fn stack(endpoint: Endpoint, address: [u8; 4], seed: u64) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, StackConfig::default(), seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new(address),
            prefix_len: 24,
            gateway: None,
        })
        .expect("ipv4");
    stack.test_set_trace(true);
    stack
}

fn inject_ce(
    stack: &mut Stack<Endpoint>,
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    payload: &[u8],
) {
    let header = TcpHeader {
        src_port,
        dst_port,
        sequence: seq,
        acknowledgement: ack,
        flags: TcpFlags::from_bits(TcpFlags::ACK.bits() | TcpFlags::PSH.bits()),
        window: 65_535,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(A), WireIpv4Addr::new(B));
    let mut segment = vec![0u8; 20 + payload.len()];
    {
        let mut builder =
            TcpBuilder::new(&mut segment, context, &header, &[], payload.len()).expect("tcp");
        builder.payload_mut().copy_from_slice(payload);
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 9,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(A),
        dst: WireIpv4Addr::new(B),
        ecn: 3,
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + segment.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &ip_header, segment.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(&segment);
    builder.finish();
    let mut frame = vec![0u8; 14 + packet.len()];
    let mut builder = EthernetBuilder::new(
        &mut frame,
        MacAddr::new(AMAC),
        MacAddr::new(BMAC),
        EtherType::Ipv4,
        packet.len(),
    )
    .expect("ethernet");
    builder.payload_mut().copy_from_slice(&packet);
    builder.finish();
    stack.test_inject_frame(&frame, Instant::ZERO);
}

#[test]
fn ce_marked_data_is_acknowledged_with_ece() {
    let (link, mut a, mut b) = pair();
    let (client, server) = common::handshake(&mut a, &mut b);
    let _ = client;
    let seq = b.test_tcp_rcv_nxt(server).expect("rcv_nxt");
    let (_, _, snd_nxt) = b.test_tcp_window(server).expect("window");
    let mark = link.trace().entries().len();
    inject_ce(&mut b, 40_000, 80, seq, snd_nxt, b"x");
    b.poll(Instant::from_millis(300));
    let ack = trace_tcp(&link, mark)
        .into_iter()
        .find(|(flags, _, _)| flags.ack() && !flags.syn())
        .expect("server ACK");
    assert!(
        ack.0.ece(),
        "CE-marked data must be acknowledged with ECE, got flags {:?}",
        ack.0
    );
}

#[test]
fn ce_marked_data_makes_the_next_data_segment_carry_cwr() {
    let (link, mut a, mut b) = pair();
    let (_client, server) = common::handshake(&mut a, &mut b);
    let seq = b.test_tcp_rcv_nxt(server).expect("rcv_nxt");
    let (_, _, snd_nxt) = b.test_tcp_window(server).expect("window");
    let mark = link.trace().entries().len();
    inject_ce(&mut b, 40_000, 80, seq, snd_nxt, b"x");
    b.poll(Instant::from_millis(300));
    b.send(server, b"z").expect("send after CE");
    b.poll(Instant::from_millis(310));
    let data = trace_tcp(&link, mark)
        .into_iter()
        .find(|(flags, _, _)| flags.psh() && flags.cwr())
        .expect("new data after CE must carry CWR");
    assert!(data.0.cwr());
}

fn trace_tcp(link: &raven_tcpip_sim::Link, start: usize) -> Vec<(TcpFlags, u32, u32)> {
    let mut out = Vec::new();
    for entry in link.trace().entries().iter().skip(start) {
        let Ok(ethernet) = raven_tcpip_wire::ethernet::EthernetFrame::parse(&entry.bytes) else {
            continue;
        };
        if ethernet.ether_type() != EtherType::Ipv4 {
            continue;
        }
        let Ok(ipv4) = raven_tcpip_wire::ipv4::Ipv4Packet::parse(ethernet.payload()) else {
            continue;
        };
        if ipv4.protocol() != 6 {
            continue;
        }
        let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
        let Ok(tcp) = raven_tcpip_wire::tcp::TcpPacket::parse(ipv4.payload(), context) else {
            continue;
        };
        out.push((tcp.flags(), tcp.sequence(), tcp.acknowledgement()));
    }
    out
}
