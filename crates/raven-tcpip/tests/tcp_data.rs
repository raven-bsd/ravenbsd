// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TCP data transfer: queues, windows, SWS, Nagle, delayed ACK and zero-window.

#![cfg(feature = "testutil")]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use raven_abi::errno::Errno;
use raven_tcpip::{
    Instant, IpAddr, Ipv4Addr, Ipv4Config, LoopbackDevice, Stack, StackConfig, TcpState,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Header, Ipv4Packet};
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpPacket};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn address(octets: [u8; 4]) -> Ipv4Addr {
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

fn pair_with(config: StackConfig) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(LinkConfig::default(), 71);
    let link = a.link();
    let mut a = endpoint_stack(a, A, 1, config);
    let mut b = endpoint_stack(b, B, 2, config);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

fn pair() -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    pair_with(StackConfig::default())
}

fn handshake(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
) -> (raven_tcpip::SocketId, raven_tcpip::SocketId) {
    b.tcp_listen(80, 8).expect("listen");
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    for time in [0, 50, 100, 150, 200] {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("connection");
    (socket, accepted)
}

fn pump(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, start: i64, steps: usize) {
    for step in 0..steps {
        let now = Instant::from_millis(start + i64::try_from(step).unwrap_or(0) * 5);
        a.poll(now);
        b.poll(now);
    }
}

fn send_all(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    send_socket: raven_tcpip::SocketId,
    recv_socket: raven_tcpip::SocketId,
    data: &[u8],
    start: i64,
) -> Vec<u8> {
    let mut sent = 0usize;
    let mut received = Vec::new();
    let mut time = start;
    let mut buffer = vec![0u8; 4096];
    for _ in 0..20_000 {
        if sent < data.len()
            && let Ok(count) = a.send(send_socket, &data[sent..])
        {
            sent += count;
        }
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        loop {
            match b.recv(recv_socket, &mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    received.extend_from_slice(&buffer[..count]);
                    if received.len() >= data.len() {
                        break;
                    }
                }
                Err(Errno::EAGAIN) => break,
                Err(_) => break,
            }
        }
        if sent == data.len() && received.len() >= data.len() {
            break;
        }
        time += 5;
    }
    received
}

fn trace_link_tcp(link: &Link, start: usize) -> Vec<(TcpFlags, u32, u32, u16)> {
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
                tcp.window(),
            ))
        })
        .collect()
}

fn inject_segment(
    stack: &mut Stack<Endpoint>,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    payload: &[u8],
) {
    let header = TcpHeader {
        src_port: 40_000,
        dst_port: 80,
        sequence: seq,
        acknowledgement: ack,
        flags,
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
        id: 7,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(A),
        dst: WireIpv4Addr::new(B),
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
fn one_byte_each_way() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, socket_b) = handshake(&mut a, &mut b);
    assert_eq!(a.send(socket_a, b"x").expect("send"), 1);
    pump(&mut a, &mut b, 210, 30);
    let mut buffer = [0u8; 1];
    assert_eq!(b.recv(socket_b, &mut buffer).expect("recv"), 1);
    assert_eq!(&buffer, b"x");
    assert_eq!(b.send(socket_b, b"y").expect("send"), 1);
    pump(&mut a, &mut b, 400, 30);
    assert_eq!(a.recv(socket_a, &mut buffer).expect("recv"), 1);
    assert_eq!(&buffer, b"y");
}

#[test]
fn one_mib_each_way_matches_a_hash() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, socket_b) = handshake(&mut a, &mut b);
    let payload: Vec<u8> = (0..1024 * 1024)
        .map(|index| (index * 31 + 7) as u8)
        .collect();
    let received = send_all(&mut a, &mut b, socket_a, socket_b, &payload, 210);
    assert_eq!(received.len(), payload.len());
    let mut before = DefaultHasher::new();
    payload.hash(&mut before);
    let mut after = DefaultHasher::new();
    received.hash(&mut after);
    assert_eq!(before.finish(), after.finish());
    let _ = socket_b;
}

#[test]
fn partial_send_and_eagain_then_resume() {
    let config = StackConfig {
        tx_queue_bytes: 8,
        ..StackConfig::default()
    };
    let (_link, mut a, mut b) = pair_with(config);
    let (socket_a, socket_b) = handshake(&mut a, &mut b);
    assert_eq!(a.send(socket_a, b"12345678").expect("full"), 8);
    assert_eq!(a.send(socket_a, b"9"), Err(Errno::EAGAIN));
    pump(&mut a, &mut b, 210, 40);
    let mut buffer = [0u8; 16];
    let mut read = 0;
    while read == 0 {
        read = b.recv(socket_b, &mut buffer).unwrap_or(0);
        pump(&mut a, &mut b, 400, 5);
    }
    assert_eq!(&buffer[..read], b"12345678");
    assert!(a.send(socket_a, b"9").is_ok());
    pump(&mut a, &mut b, 600, 40);
    assert_eq!(b.recv(socket_b, &mut buffer).expect("recv"), 1);
}

#[test]
fn recv_returns_zero_after_fin_and_data_is_drained_first() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, socket_b) = handshake(&mut a, &mut b);
    a.send(socket_a, b"tail").expect("send");
    a.shutdown_write(socket_a).expect("fin");
    pump(&mut a, &mut b, 210, 60);
    let mut buffer = [0u8; 8];
    let mut total = Vec::new();
    for _ in 0..30 {
        match b.recv(socket_b, &mut buffer) {
            Ok(0) => break,
            Ok(count) => total.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => pump(&mut a, &mut b, 600, 5),
            Err(_) => break,
        }
    }
    assert_eq!(total, b"tail");
    assert_eq!(b.recv(socket_b, &mut buffer), Ok(0));
}

#[test]
fn out_of_order_segments_reassemble() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    inject_segment(
        &mut b,
        rcv_nxt.wrapping_add(5),
        ack,
        TcpFlags::ACK,
        b"world",
    );
    inject_segment(&mut b, rcv_nxt, ack, TcpFlags::ACK, b"hello");
    let mut buffer = [0u8; 16];
    let mut total = Vec::new();
    for _ in 0..5 {
        match b.recv(socket_b, &mut buffer) {
            Ok(count) if count > 0 => total.extend_from_slice(&buffer[..count]),
            _ => break,
        }
    }
    assert_eq!(total, b"helloworld");
    let _ = a;
}

#[test]
fn duplicate_data_is_not_delivered_twice() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    inject_segment(&mut b, rcv_nxt, ack, TcpFlags::ACK, b"abc");
    inject_segment(&mut b, rcv_nxt, ack, TcpFlags::ACK, b"abc");
    let mut buffer = [0u8; 16];
    assert_eq!(b.recv(socket_b, &mut buffer).expect("recv"), 3);
    assert_eq!(&buffer[..3], b"abc");
    pump(&mut a, &mut b, 300, 20);
    assert_eq!(b.recv(socket_b, &mut buffer), Err(Errno::EAGAIN));
}

#[test]
fn data_beyond_the_window_is_dropped_and_counted() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    inject_segment(
        &mut b,
        rcv_nxt.wrapping_add(100_000),
        ack,
        TcpFlags::ACK,
        b"outside",
    );
    let mut buffer = [0u8; 16];
    assert_eq!(b.recv(socket_b, &mut buffer), Err(Errno::EAGAIN));
    assert!(b.poll(Instant::from_millis(10)).dropped >= 1);
}

#[test]
fn nagle_coalesces_and_nodelay_disables_it() {
    let (link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.send(socket_a, b"a").expect("first");
    a.send(socket_a, b"b").expect("second");
    a.poll(Instant::from_millis(210));
    let frames = trace_link_tcp(&link, start);
    let data_frames = frames
        .iter()
        .filter(|(flags, _, _, _)| flags.ack() && !flags.syn() && !flags.fin() && !flags.rst())
        .count();
    assert_eq!(data_frames, 1, "Nagle should hold the second small write");
    a.set_nodelay(socket_a, true).expect("nodelay");
    a.send(socket_a, b"c").expect("third");
    a.poll(Instant::from_millis(215));
    let frames = trace_link_tcp(&link, start);
    let data_frames = frames
        .iter()
        .filter(|(flags, _, _, _)| flags.ack() && !flags.syn() && !flags.fin() && !flags.rst())
        .count();
    assert!(data_frames >= 2, "nodelay should transmit the small write");
}

#[test]
fn urgent_pointer_is_parsed_and_counted() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let header = TcpHeader {
        src_port: 40_000,
        dst_port: 80,
        sequence: rcv_nxt,
        acknowledgement: b.test_tcp_window(socket_b).expect("window").2,
        flags: TcpFlags::from_bits(TcpFlags::ACK.bits() | TcpFlags::URG.bits()),
        window: 65_535,
        urgent_pointer: 1,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(B), WireIpv4Addr::new(A));
    let mut segment = vec![0u8; 21];
    {
        let mut builder = TcpBuilder::new(&mut segment, context, &header, &[], 1).expect("tcp");
        builder.payload_mut()[0] = b'!';
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 9,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(A),
        dst: WireIpv4Addr::new(B),
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
    b.test_inject_frame(&frame, Instant::ZERO);
    assert_eq!(b.tcp_urgent(socket_b), Ok(Some(1)));
    let (_, count) = b.test_tcp_urgent(socket_b).expect("urgent");
    assert_eq!(count, 1);
}

#[test]
fn queue_lengths_report_the_buffered_bytes() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    assert_eq!(a.send_queue_len(socket_a), Ok(0));
    a.send(socket_a, b"abc").expect("send");
    assert_eq!(a.send_queue_len(socket_a), Ok(3));
    assert_eq!(a.recv_queue_len(socket_a), Ok(0));
    assert!(a.set_nodelay(socket_a, true).is_ok());
}

#[test]
fn data_segments_carry_the_peer_window() {
    let (link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.send(socket_a, b"window").expect("send");
    a.poll(Instant::from_millis(210));
    let frames = trace_link_tcp(&link, start);
    assert!(frames.iter().any(|(_, _, _, window)| *window > 0));
}

#[test]
fn slow_reader_keeps_the_configured_caps() {
    let config = StackConfig {
        rx_queue_bytes: 4096,
        tx_queue_bytes: 4096,
        ..StackConfig::default()
    };
    let (_link, mut a, mut b) = pair_with(config);
    let (socket_a, socket_b) = handshake(&mut a, &mut b);
    let payload = vec![0x33u8; 8192];
    let mut sent = 0;
    let mut received = 0;
    let mut buffer = [0u8; 512];
    for step in 0..2000 {
        if sent < payload.len()
            && let Ok(count) = a.send(socket_a, &payload[sent..])
        {
            sent += count;
        }
        if step % 7 == 0
            && let Ok(count) = b.recv(socket_b, &mut buffer)
        {
            received += count;
        }
        let now = Instant::from_millis(210 + i64::from(step));
        a.poll(now);
        b.poll(now);
        assert!(a.send_queue_len(socket_a).unwrap_or(0) <= 4096);
        assert!(a.recv_queue_len(socket_a).unwrap_or(0) <= 4096);
        assert!(b.recv_queue_len(socket_b).unwrap_or(0) <= 4096);
    }
    assert!(received > 0);
}

#[test]
fn zero_window_stalls_and_resumes() {
    let config = StackConfig {
        rx_queue_bytes: 2048,
        ..StackConfig::default()
    };
    let (_link, mut a, mut b) = pair_with(config);
    let (socket_a, socket_b) = handshake(&mut a, &mut b);
    let payload = vec![0x44u8; 4096];
    let mut sent = 0;
    let mut time = 210;
    for _ in 0..100 {
        if sent < payload.len()
            && let Ok(count) = a.send(socket_a, &payload[sent..])
        {
            sent += count;
        }
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        time += 10;
    }
    let mut buffer = [0u8; 512];
    let mut received = 0;
    while received < 2048 {
        match b.recv(socket_b, &mut buffer) {
            Ok(count) if count > 0 => received += count,
            _ => break,
        }
    }
    assert!(received > 0);
    for _ in 0..100 {
        if sent < payload.len()
            && let Ok(count) = a.send(socket_a, &payload[sent..])
        {
            sent += count;
        }
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        time += 10;
    }
    assert!(sent > 0);
}

#[test]
fn shutdown_write_drains_before_fin() {
    let (link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    a.send(socket_a, b"last").expect("send");
    a.shutdown_write(socket_a).expect("shutdown");
    let start = link.trace().entries().len();
    pump(&mut a, &mut b, 210, 60);
    let frames = trace_link_tcp(&link, start);
    let fin_index = frames
        .iter()
        .position(|(flags, _, _, _)| flags.fin())
        .expect("FIN");
    let data_before = frames[..fin_index]
        .iter()
        .any(|(flags, _, _, _)| flags.ack() && !flags.syn());
    assert!(data_before, "data must be sent before FIN");
}

#[test]
fn send_empty_is_zero_and_recv_empty_buffer_is_zero() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    assert_eq!(a.send(socket_a, b""), Ok(0));
    assert_eq!(a.recv(socket_a, &mut []), Ok(0));
}

#[test]
fn set_nodelay_on_udp_is_enotsock() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    let udp = stack.udp_bind(53).expect("bind");
    assert_eq!(stack.set_nodelay(udp, true), Err(Errno::ENOTSOCK));
}

#[test]
fn send_queue_len_on_unknown_socket_is_ebadf() {
    let stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    assert_eq!(
        stack.send_queue_len(raven_tcpip::SocketId(99)),
        Err(Errno::EBADF)
    );
}

#[test]
fn send_on_listener_is_enotconn() {
    let (_link, _a, mut b) = pair();
    let listener = b.tcp_listen(81, 1).expect("listen");
    assert_eq!(b.send(listener, b"x"), Err(Errno::ENOTCONN));
}

#[test]
fn recv_on_listener_is_enotsock() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    let listener = stack.tcp_listen(81, 1).expect("listen");
    assert_eq!(stack.recv(listener, &mut [0u8; 1]), Err(Errno::ENOTSOCK));
}

#[test]
fn set_nodelay_on_listener_is_enotsock() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    let listener = stack.tcp_listen(81, 1).expect("listen");
    assert_eq!(stack.set_nodelay(listener, true), Err(Errno::ENOTSOCK));
}

#[test]
fn send_on_udp_is_enotconn() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    let udp = stack.udp_bind(53).expect("bind");
    assert_eq!(stack.send(udp, b"x"), Err(Errno::ENOTCONN));
}

#[test]
fn queue_length_on_udp_is_enotsock() {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        1,
        Instant::ZERO,
    );
    let udp = stack.udp_bind(53).expect("bind");
    assert_eq!(stack.send_queue_len(udp), Err(Errno::ENOTSOCK));
    assert_eq!(stack.recv_queue_len(udp), Err(Errno::ENOTSOCK));
}

#[test]
fn partial_send_returns_the_exact_room() {
    let config = StackConfig {
        tx_queue_bytes: 8,
        ..StackConfig::default()
    };
    let (_link, mut a, mut b) = pair_with(config);
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    assert_eq!(a.send(socket_a, b"abcdefghij").expect("partial"), 8);
    assert_eq!(a.send(socket_a, b"k"), Err(Errno::EAGAIN));
}

#[test]
fn recv_queue_len_tracks_reads() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    inject_segment(&mut b, rcv_nxt, ack, TcpFlags::ACK, b"queued");
    assert_eq!(b.recv_queue_len(socket_b), Ok(6));
    let mut buffer = [0u8; 3];
    assert_eq!(b.recv(socket_b, &mut buffer).expect("recv"), 3);
    assert_eq!(b.recv_queue_len(socket_b), Ok(3));
}

#[test]
fn second_full_segment_gets_an_immediate_ack() {
    let (link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    let start = link.trace().entries().len();
    inject_segment(&mut b, rcv_nxt, ack, TcpFlags::ACK, &vec![b'a'; 1460]);
    let first_acks = trace_link_tcp(&link, start)
        .iter()
        .filter(|(flags, _, _, _)| flags.ack() && !flags.syn())
        .count();
    assert!(first_acks <= 1);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    inject_segment(&mut b, rcv_nxt, ack, TcpFlags::ACK, &vec![b'b'; 1460]);
    let second_acks = trace_link_tcp(&link, start)
        .iter()
        .filter(|(flags, _, _, _)| flags.ack() && !flags.syn())
        .count();
    assert!(second_acks >= 2);
}

#[test]
fn out_of_order_hole_limit_is_bounded() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    for index in 0..12u32 {
        inject_segment(
            &mut b,
            rcv_nxt.wrapping_add(100 + index * 10),
            ack,
            TcpFlags::ACK,
            b"hole",
        );
    }
    let holes = b.test_tcp_ooo_len(socket_b).expect("holes");
    assert!(holes <= 32, "holes {holes}");
}

#[test]
fn overlapping_data_tail_is_trimmed() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    inject_segment(&mut b, rcv_nxt, ack, TcpFlags::ACK, b"abcdef");
    inject_segment(
        &mut b,
        rcv_nxt.wrapping_add(3),
        ack,
        TcpFlags::ACK,
        b"defghi",
    );
    let mut buffer = [0u8; 16];
    let mut total = Vec::new();
    for _ in 0..5 {
        match b.recv(socket_b, &mut buffer) {
            Ok(count) if count > 0 => total.extend_from_slice(&buffer[..count]),
            _ => break,
        }
    }
    assert_eq!(total, b"abcdefghi");
}

#[test]
fn zero_window_probes_fire_and_are_counted() {
    let config = StackConfig {
        rx_queue_bytes: 512,
        ..StackConfig::default()
    };
    let (_link, mut a, mut b) = pair_with(config);
    let (socket_a, socket_b) = handshake(&mut a, &mut b);
    let payload = vec![0x55u8; 2048];
    let _ = a.send(socket_a, &payload);
    let mut time = 210;
    for _ in 0..100 {
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        time += 10;
    }
    assert!(b.recv_queue_len(socket_b).unwrap_or(0) > 0);
    let mut buffer = [0u8; 512];
    let mut drained = 0;
    while let Ok(count) = b.recv(socket_b, &mut buffer) {
        if count == 0 {
            break;
        }
        drained += count;
        if drained > 1024 {
            break;
        }
    }
    let probes_before = a.test_tcp_zero_window_probes(socket_a).unwrap_or(0);
    for _ in 0..50 {
        a.poll(Instant::from_millis(time));
        time += 10;
    }
    let probes_after = a.test_tcp_zero_window_probes(socket_a).unwrap_or(0);
    assert!(probes_after >= probes_before);
}

#[test]
fn psh_is_set_on_the_last_segment_of_a_batch() {
    let (link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.send(socket_a, b"batch").expect("send");
    a.poll(Instant::from_millis(210));
    let frames = trace_link_tcp(&link, start);
    assert!(
        frames
            .iter()
            .any(|(flags, _, _, _)| flags.ack() && flags.psh())
    );
}

#[test]
fn shutdown_with_empty_queue_sends_fin_immediately() {
    let (link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.shutdown_write(socket_a).expect("shutdown");
    assert!(
        trace_link_tcp(&link, start)
            .iter()
            .any(|(flags, _, _, _)| flags.fin())
    );
}

#[test]
fn shutdown_write_twice_is_idempotent() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    a.shutdown_write(socket_a).expect("first");
    a.shutdown_write(socket_a).expect("second");
    assert_eq!(a.tcp_state(socket_a), Ok(TcpState::FinWait1));
}

#[test]
fn send_after_shutdown_is_epipe() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    a.shutdown_write(socket_a).expect("shutdown");
    assert_eq!(a.send(socket_a, b"x"), Err(Errno::EPIPE));
}

#[test]
fn tcp_state_remains_established_after_data() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    a.send(socket_a, b"data").expect("send");
    assert_eq!(a.tcp_state(socket_a), Ok(TcpState::Established));
    assert_eq!(a.recv_queue_len(socket_a), Ok(0));
}

#[test]
fn acknowledgement_frees_the_send_queue() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, socket_b) = handshake(&mut a, &mut b);
    a.send(socket_a, b"freed").expect("send");
    assert_eq!(a.send_queue_len(socket_a), Ok(5));
    for step in 0..100 {
        a.poll(Instant::from_millis(210 + step));
        b.poll(Instant::from_millis(210 + step));
        let mut buffer = [0u8; 8];
        let _ = b.recv(socket_b, &mut buffer);
        if a.send_queue_len(socket_a) == Ok(0) {
            break;
        }
    }
    assert_eq!(a.send_queue_len(socket_a), Ok(0));
}

#[test]
fn data_entirely_left_of_the_window_is_acked_not_delivered() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    inject_segment(
        &mut b,
        rcv_nxt.wrapping_sub(5),
        ack,
        TcpFlags::ACK,
        b"oldxx",
    );
    let mut buffer = [0u8; 8];
    assert_eq!(b.recv(socket_b, &mut buffer), Err(Errno::EAGAIN));
}

#[test]
fn two_urgent_segments_are_counted() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    for offset in 0..2u32 {
        let header = TcpHeader {
            src_port: 40_000,
            dst_port: 80,
            sequence: rcv_nxt.wrapping_add(offset),
            acknowledgement: ack,
            flags: TcpFlags::from_bits(TcpFlags::ACK.bits() | TcpFlags::URG.bits()),
            window: 65_535,
            urgent_pointer: 1,
        };
        let context = ChecksumContext::ipv4(WireIpv4Addr::new(A), WireIpv4Addr::new(B));
        let mut segment = vec![0u8; 21];
        {
            let mut builder = TcpBuilder::new(&mut segment, context, &header, &[], 1).expect("tcp");
            builder.payload_mut()[0] = b'u';
            builder.finish();
        }
        let ip_header = Ipv4Header {
            id: 10 + offset as u16,
            ttl: 64,
            protocol: 6,
            src: WireIpv4Addr::new(A),
            dst: WireIpv4Addr::new(B),
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
        b.test_inject_frame(&frame, Instant::ZERO);
    }
    let (_, count) = b.test_tcp_urgent(socket_b).expect("urgent");
    assert_eq!(count, 2);
}

#[test]
fn empty_send_does_not_change_queue_lengths() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    assert_eq!(a.send(socket_a, b""), Ok(0));
    assert_eq!(a.send_queue_len(socket_a), Ok(0));
    assert_eq!(a.recv_queue_len(socket_a), Ok(0));
}

#[test]
fn partial_send_accounts_for_the_exact_bytes() {
    let config = StackConfig {
        tx_queue_bytes: 6,
        ..StackConfig::default()
    };
    let (_link, mut a, mut b) = pair_with(config);
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    assert_eq!(a.send(socket_a, b"123456789").expect("partial"), 6);
    assert_eq!(a.send_queue_len(socket_a), Ok(6));
}

#[test]
fn recv_queue_length_is_zero_after_fin_and_drain() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, socket_b) = handshake(&mut a, &mut b);
    a.send(socket_a, b"x").expect("send");
    a.shutdown_write(socket_a).expect("fin");
    pump(&mut a, &mut b, 210, 80);
    let mut buffer = [0u8; 8];
    let mut drained = 0;
    for _ in 0..20 {
        match b.recv(socket_b, &mut buffer) {
            Ok(0) => break,
            Ok(count) => drained += count,
            Err(Errno::EAGAIN) => pump(&mut a, &mut b, 800, 10),
            Err(_) => break,
        }
    }
    assert_eq!(drained, 1);
    assert_eq!(b.recv_queue_len(socket_b), Ok(0));
}

#[test]
fn data_and_fin_in_one_segment_moves_to_close_wait() {
    let (_link, mut a, mut b) = pair();
    let (_socket_a, socket_b) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(socket_b).expect("rcv_nxt");
    let ack = b.test_tcp_window(socket_b).expect("window").2;
    let header = TcpHeader {
        src_port: 40_000,
        dst_port: 80,
        sequence: rcv_nxt,
        acknowledgement: ack,
        flags: TcpFlags::from_bits(TcpFlags::ACK.bits() | TcpFlags::FIN.bits()),
        window: 65_535,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(A), WireIpv4Addr::new(B));
    let mut segment = vec![0u8; 21];
    {
        let mut builder = TcpBuilder::new(&mut segment, context, &header, &[], 1).expect("tcp");
        builder.payload_mut()[0] = b'f';
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 20,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(A),
        dst: WireIpv4Addr::new(B),
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
    b.test_inject_frame(&frame, Instant::ZERO);
    assert_eq!(b.tcp_state(socket_b), Ok(TcpState::CloseWait));
    let mut buffer = [0u8; 8];
    assert_eq!(b.recv(socket_b, &mut buffer).expect("recv"), 1);
    assert_eq!(&buffer[..1], b"f");
}

#[test]
fn tcp_zero_window_probe_count_starts_at_zero() {
    let (_link, mut a, mut b) = pair();
    let (socket_a, _socket_b) = handshake(&mut a, &mut b);
    assert_eq!(a.test_tcp_zero_window_probes(socket_a), Some(0));
}
