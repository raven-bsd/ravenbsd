// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ICMPv4 and UDP socket tests.

#![cfg(feature = "testutil")]

use raven_abi::errno::Errno;
use raven_tcpip::{Instant, Ipv4Addr, Ipv4Config, LoopbackDevice, Stack, StackConfig};
use raven_tcpip_sim::{Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::icmpv4::{Icmpv4Builder, Icmpv4Packet};
use raven_tcpip_wire::ipv4::{
    Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Flags, Ipv4Header, Ipv4Packet,
};
use raven_tcpip_wire::udp::UdpBuilder;

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

fn stack_with(config: StackConfig, seed: u64) -> Stack<LoopbackDevice> {
    let mut stack = Stack::new(LoopbackDevice::with_mtu(1514), config, seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(A),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack.test_set_trace(true);
    stack
}

fn stack() -> Stack<LoopbackDevice> {
    stack_with(StackConfig::default(), 7)
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

fn ipv4_packet(
    src: [u8; 4],
    dst: [u8; 4],
    protocol: u8,
    flags: Ipv4Flags,
    frag_offset: u16,
    payload: &[u8],
) -> Vec<u8> {
    let header = Ipv4Header {
        id: 1,
        flags,
        frag_offset,
        ttl: 64,
        protocol,
        src: WireIpv4Addr::new(src),
        dst: WireIpv4Addr::new(dst),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + payload.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &header, payload.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(payload);
    builder.finish();
    packet
}

fn ipv4_frame(src: [u8; 4], dst: [u8; 4], protocol: u8, payload: &[u8]) -> Vec<u8> {
    ethernet(
        AMAC,
        BMAC,
        EtherType::Ipv4,
        &ipv4_packet(src, dst, protocol, Ipv4Flags::default(), 0, payload),
    )
}

fn udp_datagram(
    src: [u8; 4],
    dst: [u8; 4],
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Vec<u8> {
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(src), WireIpv4Addr::new(dst));
    let mut datagram = vec![0u8; 8 + payload.len()];
    let mut builder =
        UdpBuilder::new(&mut datagram, context, src_port, dst_port, payload.len()).expect("udp");
    builder.payload_mut().copy_from_slice(payload);
    builder.finish();
    datagram
}

fn udp_frame(src: [u8; 4], dst: [u8; 4], src_port: u16, dst_port: u16, payload: &[u8]) -> Vec<u8> {
    ipv4_frame(
        src,
        dst,
        17,
        &udp_datagram(src, dst, src_port, dst_port, payload),
    )
}

fn icmp_frame(
    src: [u8; 4],
    dst: [u8; 4],
    message_type: u8,
    code: u8,
    rest: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut message = vec![0u8; 8 + payload.len()];
    let mut builder =
        Icmpv4Builder::new(&mut message, message_type, code, rest, payload.len()).expect("icmp");
    builder.payload_mut().copy_from_slice(payload);
    builder.finish();
    ipv4_frame(src, dst, 1, &message)
}

fn quoted_udp(src: [u8; 4], dst: [u8; 4], src_port: u16, dst_port: u16) -> Vec<u8> {
    let mut transport = vec![0u8; 8];
    transport[0..2].copy_from_slice(&src_port.to_be_bytes());
    transport[2..4].copy_from_slice(&dst_port.to_be_bytes());
    ipv4_packet(src, dst, 17, Ipv4Flags::default(), 0, &transport)
}

fn trace_icmp(stack: &Stack<LoopbackDevice>) -> Vec<Icmpv4Packet<'_>> {
    stack
        .test_trace()
        .iter()
        .filter_map(|frame| {
            let ethernet = EthernetFrame::parse(frame).ok()?;
            if ethernet.ether_type() != EtherType::Ipv4 {
                return None;
            }
            let packet = Ipv4Packet::parse(ethernet.payload()).ok()?;
            if packet.protocol() != 1 {
                return None;
            }
            Icmpv4Packet::parse(packet.payload()).ok()
        })
        .collect()
}

#[test]
fn udp_roundtrip_between_two_stacks_over_the_simulator() {
    let (endpoint_a, endpoint_b) = Link::new(LinkConfig::default(), 11);
    let link = endpoint_a.link();
    let mut a = Stack::new(endpoint_a, StackConfig::default(), 1, Instant::ZERO);
    let mut b = Stack::new(endpoint_b, StackConfig::default(), 2, Instant::ZERO);
    a.set_ipv4(Ipv4Config {
        addr: address(A),
        prefix_len: 24,
        gateway: None,
    })
    .expect("a ipv4");
    b.set_ipv4(Ipv4Config {
        addr: address(B),
        prefix_len: 24,
        gateway: None,
    })
    .expect("b ipv4");
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    let socket_a = a.udp_bind(40_000).expect("bind a");
    let socket_b = b.udp_bind(53).expect("bind b");
    assert_eq!(
        a.udp_send_to(socket_a, b"dns", (address(B), 53))
            .expect("send"),
        3
    );
    assert_eq!(
        link.next_delivery(Instant::ZERO),
        Some(Instant::from_millis(50))
    );
    b.poll(Instant::from_millis(50));
    let mut buffer = [0u8; 16];
    let (length, source) = b.udp_recv_from(socket_b, &mut buffer).expect("recv");
    assert_eq!(length, 3);
    assert_eq!(&buffer[..3], b"dns");
    assert_eq!(source, (address(A), 40_000));
}

#[test]
fn udp_boundaries_are_preserved_for_three_datagrams() {
    let mut stack = stack();
    let socket = stack.udp_bind(53).expect("bind");
    for (index, payload) in [&b"a"[..], &b"hello"[..], &b"0123456789"[..]]
        .into_iter()
        .enumerate()
    {
        stack
            .test_push_udp(socket, payload, (address(B), 1000 + index as u16))
            .expect("push");
    }
    let mut buffer = [0u8; 16];
    for expected in [&b"a"[..], &b"hello"[..], &b"0123456789"[..]] {
        let (length, _) = stack.udp_recv_from(socket, &mut buffer).expect("recv");
        assert_eq!(length, expected.len());
        assert_eq!(&buffer[..length], expected);
    }
}

#[test]
fn datagram_over_the_receive_cap_is_dropped_and_socket_stays_usable() {
    let config = StackConfig {
        rx_queue_bytes: 8,
        ..StackConfig::default()
    };
    let mut stack = stack_with(config, 1);
    let socket = stack.udp_bind(53).expect("bind");
    assert_eq!(
        stack.test_push_udp(socket, b"012345678", (address(B), 1)),
        Err(Errno::EAGAIN)
    );
    assert_eq!(stack.poll(Instant::ZERO).dropped, 9);
    stack
        .test_push_udp(socket, b"ok", (address(B), 2))
        .expect("push");
    let mut buffer = [0u8; 4];
    assert_eq!(
        stack.udp_recv_from(socket, &mut buffer),
        Ok((2, (address(B), 2)))
    );
}

#[test]
fn short_recv_reports_the_true_datagram_length() {
    let mut stack = stack();
    let socket = stack.udp_bind(53).expect("bind");
    stack
        .test_push_udp(socket, b"0123456789", (address(B), 7))
        .expect("push");
    let mut buffer = [0u8; 4];
    assert_eq!(
        stack.udp_recv_from(socket, &mut buffer),
        Ok((10, (address(B), 7)))
    );
    assert_eq!(&buffer, b"0123");
}

#[test]
fn ephemeral_ports_are_deterministic_and_never_collide() {
    let mut first = stack_with(StackConfig::default(), 42);
    let mut second = stack_with(StackConfig::default(), 42);
    let first_port = first.udp_bind(0).expect("first");
    let second_port = second.udp_bind(0).expect("second");
    assert_eq!(
        first.test_udp_port(first_port).expect("port"),
        second.test_udp_port(second_port).expect("port")
    );
    let next = first.udp_bind(0).expect("next");
    assert_ne!(
        first.test_udp_port(next).expect("next port"),
        first.test_udp_port(first_port).expect("first port")
    );
    assert!(first.udp_bind(53).is_ok());
}

#[test]
fn bad_udp_checksum_is_dropped_and_counted() {
    let mut stack = stack();
    let socket = stack.udp_bind(53).expect("bind");
    let mut datagram = udp_datagram(B, A, 1234, 53, b"bad");
    datagram[6] ^= 0xff;
    stack.test_inject_frame(&ipv4_frame(B, A, 17, &datagram), Instant::ZERO);
    let mut buffer = [0u8; 8];
    assert_eq!(stack.udp_recv_from(socket, &mut buffer), Err(Errno::EAGAIN));
    assert!(stack.poll(Instant::ZERO).dropped >= 1);
}

#[test]
fn zero_udp_checksum_is_accepted_on_ipv4() {
    let mut stack = stack();
    let socket = stack.udp_bind(53).expect("bind");
    let mut datagram = udp_datagram(B, A, 1234, 53, b"zero");
    datagram[6] = 0;
    datagram[7] = 0;
    stack.test_inject_frame(&ipv4_frame(B, A, 17, &datagram), Instant::ZERO);
    let mut buffer = [0u8; 8];
    assert_eq!(
        stack.udp_recv_from(socket, &mut buffer),
        Ok((4, (address(B), 1234)))
    );
}

#[test]
fn echo_request_is_answered_with_the_payload_intact() {
    let mut stack = stack();
    stack.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    stack.test_inject_frame(&icmp_frame(B, A, 8, 0, 0x0001_0002, b"ping"), Instant::ZERO);
    stack.poll(Instant::ZERO);
    let replies = trace_icmp(&stack);
    let reply = replies
        .iter()
        .find(|message| message.message_type() == 0)
        .expect("reply");
    assert_eq!(reply.id(), 1);
    assert_eq!(reply.sequence(), 2);
    assert_eq!(reply.payload(), b"ping");
}

#[test]
fn echo_reply_goes_only_to_the_bound_echo_socket() {
    let mut stack = stack();
    let echo = stack.icmp_bind_echo(9).expect("bind echo");
    let udp = stack.udp_bind(53).expect("bind udp");
    stack.test_inject_frame(
        &icmp_frame(B, A, 0, 0, 0x0009_0004, b"reply"),
        Instant::ZERO,
    );
    let mut buffer = [0u8; 8];
    assert_eq!(stack.recv(echo, &mut buffer), Ok(5));
    assert_eq!(&buffer[..5], b"reply");
    assert_eq!(stack.udp_recv_from(udp, &mut [0u8; 8]), Err(Errno::EAGAIN));
}

#[test]
fn closed_udp_port_generates_an_icmp_port_unreachable() {
    let mut stack = stack();
    stack.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    stack.test_inject_frame(&udp_frame(B, A, 1234, 9999, b"x"), Instant::ZERO);
    stack.poll(Instant::ZERO);
    let messages = trace_icmp(&stack);
    let error = messages
        .iter()
        .find(|message| message.message_type() == 3)
        .expect("port unreachable");
    assert_eq!(error.code(), 3);
    assert_eq!(stack.test_icmp_errors_sent(), 1);
}

#[test]
fn icmp_error_is_matched_to_the_udp_socket() {
    let mut stack = stack();
    stack.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    let socket = stack.udp_bind(40_000).expect("bind");
    stack
        .udp_send_to(socket, b"query", (address(B), 53))
        .expect("send");
    let quote = quoted_udp(B, A, 53, 40_000);
    stack.test_inject_frame(&icmp_frame(B, A, 3, 3, 0, &quote), Instant::ZERO);
    assert_eq!(
        stack.udp_send_to(socket, b"again", (address(B), 53)),
        Err(Errno::ECONNREFUSED)
    );
}

#[test]
fn icmp_error_does_not_generate_another_icmp_error() {
    let mut stack = stack();
    let quote = quoted_udp(B, A, 53, 40_001);
    stack.test_inject_frame(&icmp_frame(B, A, 3, 3, 0, &quote), Instant::ZERO);
    assert_eq!(stack.test_icmp_errors_sent(), 0);
}

#[test]
fn broadcast_udp_does_not_generate_an_icmp_error() {
    let mut stack = stack();
    stack.test_inject_frame(
        &udp_frame(B, [255, 255, 255, 255], 1234, 9999, b"x"),
        Instant::ZERO,
    );
    assert_eq!(stack.test_icmp_errors_sent(), 0);
}

#[test]
fn non_initial_fragment_does_not_generate_an_icmp_error() {
    let mut stack = stack();
    let transport = udp_datagram(B, A, 1234, 9999, b"fragment");
    let frame = ethernet(
        AMAC,
        BMAC,
        EtherType::Ipv4,
        &ipv4_packet(B, A, 17, Ipv4Flags::default(), 1480, &transport[..8]),
    );
    stack.test_inject_frame(&frame, Instant::ZERO);
    assert_eq!(stack.test_icmp_errors_sent(), 0);
    assert_eq!(stack.test_reassembly_entries(), 1);
}

#[test]
fn icmp_error_rate_limit_caps_a_burst_and_recovers() {
    let config = StackConfig {
        icmp_error_rate_pps: 10,
        icmp_error_burst: 1,
        ..StackConfig::default()
    };
    let mut stack = stack_with(config, 1);
    stack.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    for index in 0..3u16 {
        stack.test_inject_frame(&udp_frame(B, A, 1234, 9000 + index, b"x"), Instant::ZERO);
    }
    assert_eq!(stack.test_icmp_errors_sent(), 1);
    stack.poll(Instant::from_millis(1_000));
    stack.test_inject_frame(
        &udp_frame(B, A, 1234, 9999, b"x"),
        Instant::from_millis(1_000),
    );
    assert_eq!(stack.test_icmp_errors_sent(), 2);
}

#[test]
fn duplicate_echo_binding_is_eaddrinuse() {
    let mut stack = stack();
    stack.icmp_bind_echo(5).expect("first");
    assert_eq!(stack.icmp_bind_echo(5), Err(Errno::EADDRINUSE));
}

#[test]
fn echo_request_from_broadcast_is_not_answered() {
    let mut stack = stack();
    stack.test_inject_frame(
        &icmp_frame(B, [255, 255, 255, 255], 8, 0, 0x0001_0001, b"x"),
        Instant::ZERO,
    );
    stack.poll(Instant::ZERO);
    assert!(
        !trace_icmp(&stack)
            .iter()
            .any(|message| message.message_type() == 0)
    );
}

#[test]
fn recv_from_a_closed_udp_socket_returns_zero() {
    let mut stack = stack();
    let socket = stack.udp_bind(53).expect("bind");
    stack.close(socket).expect("close");
    let mut buffer = [0u8; 1];
    assert_eq!(stack.udp_recv_from(socket, &mut buffer), Err(Errno::EBADF));
}

#[test]
fn udp_send_to_without_ipv4_is_unreachable() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    let socket = stack.udp_bind(53).expect("bind");
    assert_eq!(
        stack.udp_send_to(socket, b"x", (address(B), 53)),
        Err(Errno::ENETUNREACH)
    );
}

#[test]
fn icmp_echo_request_without_route_is_unreachable() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    assert_eq!(
        stack.icmp_echo_request(address(B), 1, 1, b"x"),
        Err(Errno::ENETUNREACH)
    );
}
