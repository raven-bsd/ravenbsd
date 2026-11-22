// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K367: a SYN-ACK carries WS only when the SYN offered it.

#![cfg(feature = "testutil")]

use raven_tcpip::{Instant, IpAddr, Ipv4Addr, Ipv4Config, Stack, StackConfig, TcpState};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Header, Ipv4Packet};
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpOption, TcpPacket};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

type OptionBytes = Vec<(u8, Vec<u8>)>;
type WireFrame = (TcpFlags, u32, u32, OptionBytes);

fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

fn endpoint_stack(endpoint: Endpoint, value: [u8; 4], seed: u64) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, StackConfig::default(), seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(value),
            prefix_len: 24,
            gateway: None,
        })
        .expect("ipv4");
    stack.test_set_trace(true);
    stack
}

fn pair() -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(LinkConfig::default(), 367);
    let link = a.link();
    let mut a = endpoint_stack(a, A, 1);
    let mut b = endpoint_stack(b, B, 2);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

#[allow(clippy::too_many_arguments)] // The parameters mirror the wire header fields.
fn frame(
    source: [u8; 4],
    destination: [u8; 4],
    source_port: u16,
    destination_port: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    options: &[u8],
) -> Vec<u8> {
    let header = TcpHeader {
        src_port: source_port,
        dst_port: destination_port,
        sequence: seq,
        acknowledgement: ack,
        flags,
        window: 65_535,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(source), WireIpv4Addr::new(destination));
    let padded = (options.len() + 3) & !3;
    let mut segment = vec![0u8; 20 + padded];
    {
        let builder = TcpBuilder::new(&mut segment, context, &header, options, 0).expect("tcp");
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 367,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(source),
        dst: WireIpv4Addr::new(destination),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + segment.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &ip_header, segment.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(&segment);
    builder.finish();
    let (source_mac, destination_mac) = if source == A {
        (AMAC, BMAC)
    } else {
        (BMAC, AMAC)
    };
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

fn tcp_frames(link: &Link) -> Vec<WireFrame> {
    link.trace()
        .entries()
        .iter()
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
            let options = tcp
                .options()
                .flatten()
                .filter_map(|option| match option {
                    TcpOption::WindowScale(scale) => Some((3, vec![scale])),
                    TcpOption::MaxSegmentSize(mss) => Some((2, mss.to_be_bytes().to_vec())),
                    TcpOption::SackPermitted => Some((4, Vec::new())),
                    TcpOption::Timestamps { .. } => Some((8, Vec::new())),
                    _ => None,
                })
                .collect();
            Some((tcp.flags(), tcp.sequence(), tcp.acknowledgement(), options))
        })
        .collect()
}

fn synack(link: &Link) -> (TcpFlags, u32, u32, Vec<(u8, Vec<u8>)>) {
    tcp_frames(link)
        .into_iter()
        .find(|(flags, _, _, _)| flags.syn() && flags.ack())
        .expect("SYN-ACK")
}

#[test]
fn plain_syn_gets_a_syn_ack_without_window_scale() {
    let (link, _a, mut b) = pair();
    b.tcp_listen(80, 4).expect("listen");
    let syn = frame(A, B, 40_000, 80, 100, 0, TcpFlags::SYN, &[2, 4, 5, 0xb4]);
    b.test_inject_frame(&syn, Instant::ZERO);
    b.poll(Instant::ZERO);
    let (_, _, _, options) = synack(&link);
    assert!(
        !options.iter().any(|(kind, _)| *kind == 3),
        "plain SYN must not get WS in SYN-ACK: {options:?}"
    );
}

#[test]
fn syn_with_window_scale_gets_ws_with_our_receive_scale() {
    let (link, _a, mut b) = pair();
    b.tcp_listen(80, 4).expect("listen");
    let syn = frame(
        A,
        B,
        40_000,
        80,
        100,
        0,
        TcpFlags::SYN,
        &[2, 4, 5, 0xb4, 3, 3, 7],
    );
    b.test_inject_frame(&syn, Instant::ZERO);
    b.poll(Instant::ZERO);
    let (_, _, _, options) = synack(&link);
    let ws = options
        .iter()
        .find(|(kind, _)| *kind == 3)
        .expect("WS in SYN-ACK");
    assert_eq!(ws.1.len(), 1);
    assert!(ws.1[0] <= 14, "our receive scale must be a valid shift");
}

#[test]
fn active_syn_ack_without_ws_leaves_scaling_disabled() {
    let (_link, mut a, _b) = pair();
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    let iss = a.test_tcp_iss(socket).expect("iss");
    let reply = frame(
        B,
        A,
        80,
        40_000,
        500,
        iss.wrapping_add(1),
        TcpFlags::from_bits(TcpFlags::SYN.bits() | TcpFlags::ACK.bits()),
        &[2, 4, 5, 0xb4],
    );
    a.test_inject_frame(&reply, Instant::from_millis(10));
    a.poll(Instant::from_millis(10));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Established));
    assert_eq!(
        a.test_tcp_negotiated_wscale(socket),
        Some(None),
        "no WS in SYN-ACK means no negotiation"
    );
    assert_eq!(a.test_tcp_window(socket).expect("window").0, 65_535);
}
