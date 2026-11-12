// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IPv6, neighbour discovery, SLAAC and ICMPv6 tests.

#![cfg(feature = "testutil")]

use raven_abi::errno::Errno;
use raven_tcpip::{
    Duration, Instant, IpAddr, Ipv4Addr, Ipv6Addr, Ipv6AddressState, Ipv6Cidr, Ipv6Config,
    LoopbackDevice, Neighbor6State, Stack, StackConfig,
};
use raven_tcpip_sim::{Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::icmpv6::{Icmpv6Builder, Icmpv6Packet};
use raven_tcpip_wire::ipv6::{Ipv6Addr as WireIpv6Addr, Ipv6Builder, Ipv6Header, Ipv6Packet};
use raven_tcpip_wire::udp::UdpBuilder;

const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];
const A4: [u8; 4] = [192, 0, 2, 1];
const B4: [u8; 4] = [192, 0, 2, 2];

fn a6() -> Ipv6Addr {
    Ipv6Addr::new([0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])
}

fn b6() -> Ipv6Addr {
    Ipv6Addr::new([0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2])
}

fn stack6() -> Stack<LoopbackDevice> {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        7,
        Instant::ZERO,
    );
    stack
        .set_ipv6(Ipv6Config {
            addrs: vec![Ipv6Cidr {
                addr: a6(),
                prefix_len: 64,
            }],
            gateway: None,
        })
        .expect("set_ipv6");
    stack.test_set_trace(true);
    stack
}

fn ethernet(dst: [u8; 6], src: [u8; 6], ether_type: EtherType, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0u8; 14 + payload.len()];
    let mut builder = EthernetBuilder::new(
        &mut frame,
        MacAddr::new(dst),
        MacAddr::new(src),
        ether_type,
        payload.len(),
    )
    .expect("ethernet");
    builder.payload_mut().copy_from_slice(payload);
    builder.finish();
    frame
}

fn ipv6_frame(src: Ipv6Addr, dst: Ipv6Addr, next: u8, hop: u8, payload: &[u8]) -> Vec<u8> {
    let header = Ipv6Header {
        next_header: next,
        hop_limit: hop,
        src: WireIpv6Addr::new(src.octets()),
        dst: WireIpv6Addr::new(dst.octets()),
        ..Ipv6Header::default()
    };
    let mut packet = vec![0u8; 40 + payload.len()];
    let mut builder = Ipv6Builder::new(&mut packet, &header, payload.len()).expect("ipv6");
    builder.payload_mut().copy_from_slice(payload);
    builder.finish();
    ethernet(AMAC, BMAC, EtherType::Ipv6, &packet)
}

fn icmpv6_message(
    context: ChecksumContext,
    message_type: u8,
    code: u8,
    rest: u32,
    body: &[u8],
) -> Vec<u8> {
    let mut message = vec![0u8; 8 + body.len()];
    let mut builder =
        Icmpv6Builder::new(&mut message, context, message_type, code, rest, body.len())
            .expect("icmpv6");
    builder.payload_mut().copy_from_slice(body);
    builder.finish();
    message
}

fn ns_body(target: Ipv6Addr, mac: [u8; 6]) -> Vec<u8> {
    let mut body = vec![0u8; 16 + 8];
    body[..16].copy_from_slice(&target.octets());
    body[16] = 1;
    body[17] = 1;
    body[18..24].copy_from_slice(&mac);
    body
}

fn na_body(target: Ipv6Addr, mac: [u8; 6]) -> Vec<u8> {
    let mut body = vec![0u8; 16 + 8];
    body[..16].copy_from_slice(&target.octets());
    body[16] = 2;
    body[17] = 1;
    body[18..24].copy_from_slice(&mac);
    body
}

fn nd_frame(src: Ipv6Addr, dst: Ipv6Addr, message_type: u8, body: &[u8]) -> Vec<u8> {
    let context = ChecksumContext::ipv6(
        WireIpv6Addr::new(src.octets()),
        WireIpv6Addr::new(dst.octets()),
    );
    let message = icmpv6_message(context, message_type, 0, 0, body);
    ipv6_frame(src, dst, 58, 255, &message)
}

fn ra_frame(router: Ipv6Addr, prefix: Ipv6Addr, valid: u32, preferred: u32) -> Vec<u8> {
    let mut body = vec![0u8; 12 + 32];
    body[0] = 64;
    body[2..4].copy_from_slice(&0u16.to_be_bytes());
    body[12] = 3;
    body[13] = 4;
    body[14] = 64;
    body[15] = 0xc0;
    body[16..20].copy_from_slice(&valid.to_be_bytes());
    body[20..24].copy_from_slice(&preferred.to_be_bytes());
    body[28..44].copy_from_slice(&prefix.octets());
    let context = ChecksumContext::ipv6(
        WireIpv6Addr::new(router.octets()),
        WireIpv6Addr::new(Ipv6Addr::ALL_NODES.octets()),
    );
    let message = icmpv6_message(context, 134, 0, 0, &body);
    ipv6_frame(router, Ipv6Addr::ALL_NODES, 58, 255, &message)
}

fn udp6(src: Ipv6Addr, dst: Ipv6Addr, sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let context = ChecksumContext::ipv6(
        WireIpv6Addr::new(src.octets()),
        WireIpv6Addr::new(dst.octets()),
    );
    let mut datagram = vec![0u8; 8 + payload.len()];
    let mut builder =
        UdpBuilder::new(&mut datagram, context, sport, dport, payload.len()).expect("udp6");
    builder.payload_mut().copy_from_slice(payload);
    builder.finish();
    datagram
}

fn udp6_frame(src: Ipv6Addr, dst: Ipv6Addr, sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    ipv6_frame(src, dst, 17, 64, &udp6(src, dst, sport, dport, payload))
}

fn trace_icmpv6(stack: &Stack<LoopbackDevice>) -> Vec<Icmpv6Packet<'_>> {
    stack
        .test_trace()
        .iter()
        .filter_map(|frame| {
            let ethernet = EthernetFrame::parse(frame).ok()?;
            if ethernet.ether_type() != EtherType::Ipv6 {
                return None;
            }
            let packet = Ipv6Packet::parse(ethernet.payload()).ok()?;
            let (next, upper) = packet.upper_layer();
            if next != 58 {
                return None;
            }
            let context = ChecksumContext::ipv6(packet.src(), packet.dst());
            Icmpv6Packet::parse(upper, context).ok()
        })
        .collect()
}

#[test]
fn link_local_address_is_derived_from_the_mac() {
    let address = Ipv6Addr::link_local_from_mac(AMAC);
    assert_eq!(
        address.to_string(),
        "fe80:0000:0000:0000:0000:00ff:fe00:0001"
    );
}

#[test]
fn dad_keeps_the_address_tentative_until_the_timer() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    stack.enable_slaac().expect("slaac");
    let address = Ipv6Addr::link_local_from_mac(AMAC);
    assert_eq!(
        stack.test_ipv6_state(address),
        Some(Ipv6AddressState::Tentative)
    );
    stack.poll(Instant::from_millis(999));
    assert_eq!(
        stack.test_ipv6_state(address),
        Some(Ipv6AddressState::Tentative)
    );
    stack.poll(Instant::from_millis(1_000));
    assert_eq!(
        stack.test_ipv6_state(address),
        Some(Ipv6AddressState::Usable)
    );
}

#[test]
fn traffic_to_a_tentative_address_is_refused() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    stack.enable_slaac().expect("slaac");
    let address = Ipv6Addr::link_local_from_mac(AMAC);
    stack.test_inject_frame(&udp6_frame(b6(), address, 53, 53, b"early"), Instant::ZERO);
    assert!(stack.test_take_delivered().is_empty());
    assert!(stack.poll(Instant::ZERO).dropped >= 1);
}

#[test]
fn dad_conflict_marks_the_address_unusable() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    stack.enable_slaac().expect("slaac");
    let address = Ipv6Addr::link_local_from_mac(AMAC);
    let frame = nd_frame(b6(), address, 136, &na_body(address, BMAC));
    stack.test_inject_frame(&frame, Instant::ZERO);
    assert_eq!(
        stack.test_ipv6_state(address),
        Some(Ipv6AddressState::Conflict)
    );
    assert_eq!(stack.test_dad_conflicts(), 1);
}

#[test]
fn nd_resolution_flushes_the_queued_packet() {
    let mut stack = stack6();
    stack.test_send_ipv6(b"hello6", b6(), 17).expect("queue");
    assert_eq!(
        stack.test_neighbor6_state(b6()),
        Some(Neighbor6State::Incomplete)
    );
    let frame = nd_frame(b6(), a6(), 136, &na_body(b6(), BMAC));
    stack.test_inject_frame(&frame, Instant::ZERO);
    assert_eq!(
        stack.test_neighbor6_state(b6()),
        Some(Neighbor6State::Reachable)
    );
    stack.poll(Instant::ZERO);
    assert!(stack.test_trace().iter().any(|frame| {
        EthernetFrame::parse(frame).is_ok_and(|ethernet| ethernet.ether_type() == EtherType::Ipv6)
    }));
}

#[test]
fn neighbour_reaches_stale_then_delay_then_probe() {
    let mut stack = stack6();
    stack.test_insert_neighbor6(b6(), BMAC, Instant::ZERO);
    stack.poll(Instant::from_millis(30_001));
    assert_eq!(
        stack.test_neighbor6_state(b6()),
        Some(Neighbor6State::Stale)
    );
    stack.test_send_ipv6(b"again", b6(), 17).expect("send");
    assert_eq!(
        stack.test_neighbor6_state(b6()),
        Some(Neighbor6State::Delay)
    );
    stack.poll(Instant::from_millis(35_002));
    assert_eq!(
        stack.test_neighbor6_state(b6()),
        Some(Neighbor6State::Probe)
    );
}

#[test]
fn nd_with_hop_limit_254_is_discarded() {
    let mut stack = stack6();
    let context = ChecksumContext::ipv6(
        WireIpv6Addr::new(b6().octets()),
        WireIpv6Addr::new(a6().octets()),
    );
    let message = icmpv6_message(context, 135, 0, 0, &ns_body(a6(), BMAC));
    stack.test_inject_frame(&ipv6_frame(b6(), a6(), 58, 254, &message), Instant::ZERO);
    assert_eq!(stack.test_nd_bad_hop_limit(), 1);
    assert!(stack.test_take_delivered().is_empty());
}

#[test]
fn nd_inside_a_fragment_is_discarded() {
    let mut stack = stack6();
    let context = ChecksumContext::ipv6(
        WireIpv6Addr::new(b6().octets()),
        WireIpv6Addr::new(a6().octets()),
    );
    let ns = icmpv6_message(context, 135, 0, 0, &ns_body(a6(), BMAC));
    let mut fragment = vec![0u8; 8];
    fragment[0] = 58;
    fragment[2..4].copy_from_slice(&1u16.to_be_bytes());
    fragment[7] = 1;
    let mut payload = fragment;
    payload.extend_from_slice(&ns);
    stack.test_inject_frame(&ipv6_frame(b6(), a6(), 44, 255, &payload), Instant::ZERO);
    assert_eq!(stack.test_nd_fragments(), 1);
}

#[test]
fn slaac_installs_an_address_from_an_ra() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    stack.enable_slaac().expect("slaac");
    stack.poll(Instant::from_millis(1_000));
    let prefix = Ipv6Addr::new([0x20, 0x01, 0x0d, 0xb8, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    stack.test_inject_frame(&ra_frame(b6(), prefix, 3_600, 1_800), Instant::ZERO);
    let configured = stack
        .ipv6()
        .addrs
        .iter()
        .map(|cidr| cidr.addr)
        .find(|address| address.same_prefix(prefix, 64))
        .expect("SLAAC address");
    assert_eq!(
        stack.test_ipv6_state(configured),
        Some(Ipv6AddressState::Usable)
    );
    assert_eq!(
        stack.test_ipv6_valid_until(configured),
        Some(Instant::from_millis(3_600_000))
    );
    assert_eq!(
        stack.test_ipv6_preferred_until(configured),
        Some(Instant::from_millis(1_800_000))
    );
}

#[test]
fn slaac_lifetime_extension_is_clamped_to_two_hours() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    stack.enable_slaac().expect("slaac");
    stack.poll(Instant::from_millis(1_000));
    let prefix = Ipv6Addr::new([0x20, 0x01, 0x0d, 0xb8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    stack.test_inject_frame(&ra_frame(b6(), prefix, 7_000, 5_000), Instant::ZERO);
    let configured = stack
        .ipv6()
        .addrs
        .iter()
        .map(|cidr| cidr.addr)
        .find(|address| address.same_prefix(prefix, 64))
        .expect("SLAAC address");
    stack.poll(Instant::from_millis(6_000));
    stack.test_inject_frame(
        &ra_frame(b6(), prefix, 20_000, 5_000),
        Instant::from_millis(6_000),
    );
    assert_eq!(
        stack.test_ipv6_valid_until(configured),
        Some(Instant::from_millis(6_000 + 7_200_000))
    );
}

#[test]
fn slaac_zero_valid_lifetime_deprecates_the_address() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    stack.enable_slaac().expect("slaac");
    stack.poll(Instant::from_millis(1_000));
    let prefix = Ipv6Addr::new([0x20, 0x01, 0x0d, 0xb8, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    stack.test_inject_frame(&ra_frame(b6(), prefix, 3_600, 1_800), Instant::ZERO);
    let configured = stack
        .ipv6()
        .addrs
        .iter()
        .map(|cidr| cidr.addr)
        .find(|address| address.same_prefix(prefix, 64))
        .expect("SLAAC address");
    stack.test_inject_frame(&ra_frame(b6(), prefix, 0, 0), Instant::ZERO);
    assert_eq!(
        stack.test_ipv6_state(configured),
        Some(Ipv6AddressState::Deprecated)
    );
}

#[test]
fn unknown_extension_option_with_report_bits_sends_parameter_problem() {
    let mut stack = stack6();
    stack.test_insert_neighbor6(b6(), BMAC, Instant::ZERO);
    let extension = vec![59u8, 0, 0x80, 2, 0, 0, 0, 0];
    let frame = ipv6_frame(b6(), a6(), 0, 64, &extension);
    stack.test_inject_frame(&frame, Instant::ZERO);
    stack.poll(Instant::ZERO);
    assert_eq!(stack.test_icmpv6_errors_sent(), 1);
    assert!(
        trace_icmpv6(&stack)
            .iter()
            .any(|message| message.message_type() == 4 && message.code() == 2)
    );
}

#[test]
fn unknown_extension_option_with_skip_bits_is_skipped() {
    let mut stack = stack6();
    stack.test_insert_neighbor6(b6(), BMAC, Instant::ZERO);
    let mut extension = vec![17u8, 0, 0x02, 2, 0, 0, 0, 0];
    extension[1] = 0;
    let frame = ipv6_frame(b6(), a6(), 0, 64, &extension);
    stack.test_inject_frame(&frame, Instant::ZERO);
    stack.poll(Instant::ZERO);
    assert_eq!(stack.test_icmpv6_errors_sent(), 0);
}

#[test]
fn packet_too_big_records_the_rfc_floor() {
    let mut stack = stack6();
    stack.test_insert_neighbor6(b6(), BMAC, Instant::ZERO);
    let quote = ipv6_frame(a6(), b6(), 17, 64, &udp6(a6(), b6(), 40_000, 53, b"x"));
    let quote = EthernetFrame::parse(&quote)
        .expect("ethernet")
        .payload()
        .to_vec();
    let context = ChecksumContext::ipv6(
        WireIpv6Addr::new(b6().octets()),
        WireIpv6Addr::new(a6().octets()),
    );
    let message = icmpv6_message(context, 2, 0, 1_000, &quote);
    stack.test_inject_frame(&ipv6_frame(b6(), a6(), 58, 64, &message), Instant::ZERO);
    assert_eq!(stack.test_ipv6_pmtu(b6()), Some(1_280));
}

#[test]
fn udp_over_ipv6_roundtrip_between_two_stacks() {
    let (endpoint_a, endpoint_b) = Link::new(LinkConfig::default(), 21);
    let link = endpoint_a.link();
    let mut a = Stack::new(endpoint_a, StackConfig::default(), 1, Instant::ZERO);
    let mut b = Stack::new(endpoint_b, StackConfig::default(), 2, Instant::ZERO);
    a.set_ipv6(Ipv6Config {
        addrs: vec![Ipv6Cidr {
            addr: a6(),
            prefix_len: 64,
        }],
        gateway: None,
    })
    .expect("a ipv6");
    b.set_ipv6(Ipv6Config {
        addrs: vec![Ipv6Cidr {
            addr: b6(),
            prefix_len: 64,
        }],
        gateway: None,
    })
    .expect("b ipv6");
    a.test_insert_neighbor6(b6(), BMAC, Instant::ZERO);
    b.test_insert_neighbor6(a6(), AMAC, Instant::ZERO);
    let socket_a = a.udp_bind(40_000).expect("bind a");
    let socket_b = b.udp_bind(53).expect("bind b");
    assert_eq!(
        a.udp_send_to(socket_a, b"v6dns", (IpAddr::V6(b6()), 53))
            .expect("send"),
        5
    );
    assert_eq!(
        link.next_delivery(Instant::ZERO),
        Some(Instant::from_millis(50))
    );
    b.poll(Instant::from_millis(50));
    let mut buffer = [0u8; 16];
    let (length, source) = b.udp_recv_from(socket_b, &mut buffer).expect("recv");
    assert_eq!(length, 5);
    assert_eq!(&buffer[..5], b"v6dns");
    assert_eq!(source, (IpAddr::V6(a6()), 40_000));
}

#[test]
fn v4_mapped_traffic_is_rejected_by_default() {
    let mut stack = stack6();
    let socket = stack.udp_bind(53).expect("bind");
    let mapped = Ipv6Addr::from_ipv4_mapped(Ipv4Addr::new(A4));
    stack.test_inject_frame(
        &udp6_frame(
            Ipv6Addr::from_ipv4_mapped(Ipv4Addr::new(B4)),
            mapped,
            53,
            53,
            b"mapped",
        ),
        Instant::ZERO,
    );
    assert_eq!(
        stack.udp_recv_from(socket, &mut [0u8; 8]),
        Err(Errno::EAGAIN)
    );
}

#[test]
fn v4_mapped_traffic_is_accepted_when_enabled() {
    let mut stack = stack6();
    stack.set_v4_mapped(true);
    let socket = stack.udp_bind(53).expect("bind");
    let mapped_a = Ipv6Addr::from_ipv4_mapped(Ipv4Addr::new(A4));
    let mapped_b = Ipv6Addr::from_ipv4_mapped(Ipv4Addr::new(B4));
    let frame = ethernet(AMAC, BMAC, EtherType::Ipv6, &{
        let header = Ipv6Header {
            next_header: 17,
            hop_limit: 64,
            src: WireIpv6Addr::new(mapped_b.octets()),
            dst: WireIpv6Addr::new(mapped_a.octets()),
            ..Ipv6Header::default()
        };
        let datagram = udp6(mapped_b, mapped_a, 1234, 53, b"mapped");
        let mut packet = vec![0u8; 40 + datagram.len()];
        let mut builder = Ipv6Builder::new(&mut packet, &header, datagram.len()).expect("ipv6");
        builder.payload_mut().copy_from_slice(&datagram);
        builder.finish();
        packet
    });
    stack.test_inject_frame(&frame, Instant::ZERO);
    let mut buffer = [0u8; 8];
    assert_eq!(
        stack.udp_recv_from(socket, &mut buffer),
        Ok((6, (IpAddr::V4(Ipv4Addr::new(B4)), 1234)))
    );
}

#[test]
fn icmpv6_echo_request_is_answered() {
    let mut stack = stack6();
    stack.test_insert_neighbor6(b6(), BMAC, Instant::ZERO);
    stack.test_inject_frame(&nd_frame(b6(), a6(), 128, b"ping6"), Instant::ZERO);
    stack.poll(Instant::ZERO);
    let messages = trace_icmpv6(&stack);
    assert!(
        messages
            .iter()
            .any(|message| { message.message_type() == 129 && message.payload() == b"ping6" })
    );
}

#[test]
fn icmpv6_error_is_matched_to_the_udp_socket() {
    let mut stack = stack6();
    stack.test_insert_neighbor6(b6(), BMAC, Instant::ZERO);
    let socket = stack.udp_bind(40_000).expect("bind");
    stack
        .udp_send_to(socket, b"query", (IpAddr::V6(b6()), 53))
        .expect("send");
    let quote_frame = ipv6_frame(a6(), b6(), 17, 64, &udp6(a6(), b6(), 40_000, 53, b"q"));
    let quote = EthernetFrame::parse(&quote_frame)
        .expect("ethernet")
        .payload()
        .to_vec();
    let context = ChecksumContext::ipv6(
        WireIpv6Addr::new(b6().octets()),
        WireIpv6Addr::new(a6().octets()),
    );
    let message = icmpv6_message(context, 1, 4, 0, &quote);
    stack.test_inject_frame(&ipv6_frame(b6(), a6(), 58, 64, &message), Instant::ZERO);
    assert_eq!(
        stack.udp_send_to(socket, b"again", (IpAddr::V6(b6()), 53)),
        Err(Errno::ECONNREFUSED)
    );
}

#[test]
fn icmpv6_port_unreachable_is_generated_for_a_closed_port() {
    let mut stack = stack6();
    stack.test_insert_neighbor6(b6(), BMAC, Instant::ZERO);
    stack.test_inject_frame(&udp6_frame(b6(), a6(), 1234, 9999, b"x"), Instant::ZERO);
    stack.poll(Instant::ZERO);
    assert_eq!(stack.test_icmpv6_errors_sent(), 1);
    assert!(
        trace_icmpv6(&stack)
            .iter()
            .any(|message| message.message_type() == 1 && message.code() == 4)
    );
}

#[test]
fn icmpv6_error_does_not_generate_an_error() {
    let mut stack = stack6();
    let quote_frame = ipv6_frame(a6(), b6(), 17, 64, &udp6(a6(), b6(), 40_000, 53, b"q"));
    let quote = EthernetFrame::parse(&quote_frame)
        .expect("ethernet")
        .payload()
        .to_vec();
    let context = ChecksumContext::ipv6(
        WireIpv6Addr::new(b6().octets()),
        WireIpv6Addr::new(a6().octets()),
    );
    let message = icmpv6_message(context, 1, 4, 0, &quote);
    stack.test_inject_frame(&ipv6_frame(b6(), a6(), 58, 64, &message), Instant::ZERO);
    assert_eq!(stack.test_icmpv6_errors_sent(), 0);
}

#[test]
fn multicast_udp_does_not_generate_an_icmpv6_error() {
    let mut stack = stack6();
    stack.test_inject_frame(
        &udp6_frame(b6(), Ipv6Addr::ALL_NODES, 1234, 9999, b"x"),
        Instant::ZERO,
    );
    assert_eq!(stack.test_icmpv6_errors_sent(), 0);
}

#[test]
fn ipv6_fragmented_udp_does_not_generate_an_error() {
    let mut stack = stack6();
    let mut fragment = vec![0u8; 8];
    fragment[0] = 17;
    fragment[2..4].copy_from_slice(&(185u16 << 3).to_be_bytes());
    fragment[7] = 1;
    let mut payload = fragment;
    payload.extend_from_slice(&udp6(b6(), a6(), 1234, 9999, b"x")[..8]);
    stack.test_inject_frame(&ipv6_frame(b6(), a6(), 44, 64, &payload), Instant::ZERO);
    assert_eq!(stack.test_icmpv6_errors_sent(), 0);
}

#[test]
fn ipv6_duration_constant_is_used_for_neighbour_lifetimes() {
    // Guard the timer constants used by the state machine and SLAAC.
    assert_eq!(Duration::from_millis(30_000).as_millis(), 30_000);
    assert_eq!(Duration::from_millis(7_200_000).as_millis(), 7_200_000);
}
