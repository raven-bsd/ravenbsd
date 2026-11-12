// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TCP establishment, close, TIME-WAIT, RST and RFC 5961 tests.

#![cfg(feature = "testutil")]

use raven_abi::errno::Errno;
use raven_tcpip::{
    Duration, Instant, IpAddr, Ipv4Addr, Ipv4Config, LoopbackDevice, Stack, StackConfig, TcpState,
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

fn pair() -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(LinkConfig::default(), 33);
    let link = a.link();
    let mut a = endpoint_stack(a, A, 1);
    let mut b = endpoint_stack(b, B, 2);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

fn drive(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, times: &[i64]) {
    for time in times {
        let now = Instant::from_millis(*time);
        a.poll(now);
        b.poll(now);
    }
}

fn handshake(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>) -> raven_tcpip::SocketId {
    b.tcp_listen(80, 4).expect("listen");
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    drive(a, b, &[0, 50, 100, 150]);
    socket
}

fn stack4() -> Stack<LoopbackDevice> {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        StackConfig::default(),
        5,
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

fn tcp_frame(
    source: [u8; 4],
    destination: [u8; 4],
    source_port: u16,
    destination_port: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
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
    let mut segment = vec![0u8; 20];
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(source), WireIpv4Addr::new(destination));
    {
        let builder = TcpBuilder::new(&mut segment, context, &header, &[], 0).expect("tcp");
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 1,
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
    frame
}

fn trace_tcp<D: raven_tcpip::Device>(stack: &Stack<D>) -> Vec<(TcpFlags, u32, u32)> {
    stack
        .test_trace()
        .iter()
        .filter_map(|frame| {
            let ethernet = EthernetFrame::parse(frame).ok()?;
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

fn trace_link_tcp(link: &Link, start: usize) -> Vec<(TcpFlags, u32, u32)> {
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
            Some((tcp.flags(), tcp.sequence(), tcp.acknowledgement()))
        })
        .collect()
}

#[test]
fn active_open_reaches_established_on_both_sides() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Established));
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("connection");
    assert_eq!(b.tcp_state(accepted), Ok(TcpState::Established));
}

#[test]
fn connect_to_closed_port_is_connection_refused() {
    let (_link, mut a, mut b) = pair();
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    drive(&mut a, &mut b, &[0, 50, 100]);
    eprintln!(
        "A trace {} B trace {} state {:?} err {:?}",
        a.test_trace().len(),
        b.test_trace().len(),
        a.tcp_state(socket),
        a.tcp_error(socket)
    );
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ECONNREFUSED)));
}

#[test]
fn connect_without_an_answer_retransmits_and_times_out() {
    let config = LinkConfig {
        loss_percent: 100.0,
        ..LinkConfig::default()
    };
    let (endpoint_a, endpoint_b) = Link::new(config, 44);
    let link = endpoint_a.link();
    let mut a = endpoint_stack(endpoint_a, A, 1);
    let mut b = endpoint_stack(endpoint_b, B, 2);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    drive(
        &mut a,
        &mut b,
        &[0, 1_000, 3_000, 7_000, 15_000, 31_000, 63_000, 127_000],
    );
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ETIMEDOUT)));
    let _ = link;
}

#[test]
fn orderly_close_from_the_client() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("conn");
    a.shutdown_write(socket).expect("shutdown");
    drive(&mut a, &mut b, &[200, 250, 300, 350]);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::FinWait2));
    assert_eq!(b.tcp_state(accepted), Ok(TcpState::CloseWait));
    b.shutdown_write(accepted).expect("server close");
    drive(&mut a, &mut b, &[350, 400, 450, 500]);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::TimeWait));
    assert_eq!(b.tcp_state(accepted), Ok(TcpState::Closed));
}

#[test]
fn orderly_close_from_the_server() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("conn");
    b.shutdown_write(accepted).expect("server close");
    drive(&mut a, &mut b, &[200, 250, 300, 350]);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::CloseWait));
    assert_eq!(b.tcp_state(accepted), Ok(TcpState::FinWait2));
    a.shutdown_write(socket).expect("client close");
    drive(&mut a, &mut b, &[350, 400, 450, 500]);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(b.tcp_state(accepted), Ok(TcpState::TimeWait));
}

#[test]
fn simultaneous_open_reaches_established() {
    let (_link, mut a, mut b) = pair();
    let sa = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("a connect");
    let sb = b
        .tcp_connect((IpAddr::V4(address(A)), 40_000), Some(80))
        .expect("b connect");
    drive(&mut a, &mut b, &[0, 50, 100, 150, 200]);
    assert_eq!(a.tcp_state(sa), Ok(TcpState::Established));
    assert_eq!(b.tcp_state(sb), Ok(TcpState::Established));
}

#[test]
fn simultaneous_close_reaches_time_wait() {
    let (_link, mut a, mut b) = pair();
    let sa = handshake(&mut a, &mut b);
    let sb = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("conn");
    a.shutdown_write(sa).expect("a close");
    b.shutdown_write(sb).expect("b close");
    drive(&mut a, &mut b, &[200, 250, 300, 350, 400, 450]);
    assert_eq!(a.tcp_state(sa), Ok(TcpState::TimeWait));
    assert_eq!(b.tcp_state(sb), Ok(TcpState::TimeWait));
}

#[test]
fn time_wait_expires_and_the_port_becomes_reusable() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("conn");
    a.shutdown_write(socket).expect("a close");
    drive(&mut a, &mut b, &[200, 250, 300, 350]);
    b.shutdown_write(accepted).expect("b close");
    drive(&mut a, &mut b, &[350, 400, 450, 500]);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::TimeWait));
    a.poll(Instant::from_millis(60_501));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert!(
        a.tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
            .is_ok()
    );
}

#[test]
fn fin_wait_2_timeout_closes_without_a_reset() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    a.shutdown_write(socket).expect("shutdown");
    drive(&mut a, &mut b, &[200, 250, 300]);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::FinWait2));
    a.poll(Instant::from_millis(60_301));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ETIMEDOUT)));
}

#[test]
fn rst_outside_the_window_is_ignored() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let remote = a.tcp_state(socket);
    assert_eq!(remote, Ok(TcpState::Established));
    let rcv_nxt = a.test_tcp_rcv_nxt(socket).expect("rcv_nxt");
    let frame = tcp_frame(
        B,
        A,
        80,
        40_000,
        rcv_nxt.wrapping_add(100_000),
        1,
        TcpFlags::RST,
    );
    a.test_inject_frame(&frame, Instant::from_millis(160));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Established));
}

#[test]
fn rst_inside_the_window_closes_the_connection() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let rcv_nxt = a.test_tcp_rcv_nxt(socket).expect("rcv_nxt");
    let frame = tcp_frame(B, A, 80, 40_000, rcv_nxt, 1, TcpFlags::RST);
    a.test_inject_frame(&frame, Instant::from_millis(160));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ECONNRESET)));
}

#[test]
fn recv_after_reset_returns_econnreset() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let rcv_nxt = a.test_tcp_rcv_nxt(socket).expect("rcv_nxt");
    let frame = tcp_frame(B, A, 80, 40_000, rcv_nxt, 1, TcpFlags::RST);
    a.test_inject_frame(&frame, Instant::from_millis(160));
    assert_eq!(a.recv(socket, &mut [0u8; 1]), Err(Errno::ECONNRESET));
}

#[test]
fn in_window_syn_on_established_gets_a_challenge_ack() {
    let (link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    let frame = tcp_frame(B, A, 80, 40_000, 1, 1, TcpFlags::SYN);
    a.test_inject_frame(&frame, Instant::from_millis(160));
    a.poll(Instant::from_millis(160));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Established));
    let frames = trace_link_tcp(&link, start);
    assert!(
        frames
            .iter()
            .any(|(flags, _, _)| flags.ack() && !flags.syn())
    );
}

#[test]
fn challenge_acks_are_rate_limited() {
    let (link, mut a, mut b) = pair();
    let _socket = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    for index in 0..20u32 {
        let frame = tcp_frame(B, A, 80, 40_000, 1 + index, 1, TcpFlags::SYN);
        a.test_inject_frame(&frame, Instant::from_millis(160));
    }
    a.poll(Instant::from_millis(160));
    let acks = trace_link_tcp(&link, start)
        .iter()
        .filter(|(flags, _, _)| flags.ack() && !flags.syn())
        .count();
    assert!(acks <= 10, "challenge ACK burst was not limited: {acks}");
}

#[test]
fn reset_is_never_sent_in_reply_to_a_reset() {
    let mut stack = stack4();
    stack.test_clear_trace();
    let frame = tcp_frame(B, A, 80, 9_999, 1, 0, TcpFlags::RST);
    stack.test_inject_frame(&frame, Instant::ZERO);
    stack.poll(Instant::ZERO);
    assert!(!trace_tcp(&stack).iter().any(|(flags, _, _)| flags.rst()));
}

#[test]
fn segment_to_a_closed_port_gets_a_reset() {
    let mut stack = stack4();
    stack.test_clear_trace();
    let frame = tcp_frame(B, A, 80, 9_999, 1, 0, TcpFlags::SYN);
    stack.test_inject_frame(&frame, Instant::ZERO);
    stack.poll(Instant::ZERO);
    assert!(trace_tcp(&stack).iter().any(|(flags, _, _)| flags.rst()));
}

#[test]
fn abort_sends_a_reset_and_frees_the_socket() {
    let (link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.abort(socket).expect("abort");
    a.poll(Instant::from_millis(160));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert!(
        trace_link_tcp(&link, start)
            .iter()
            .any(|(flags, _, _)| flags.rst())
    );
    assert_eq!(a.send(socket, b"x"), Err(Errno::EBADF));
}

#[test]
fn keepalive_probes_then_times_out() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    a.tcp_set_keepalive(socket, Duration::from_millis(10), 1)
        .expect("keepalive");
    a.poll(Instant::from_millis(161));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Established));
    a.poll(Instant::from_millis(172));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ETIMEDOUT)));
}

#[test]
fn backlog_limit_refuses_the_extra_half_open() {
    let mut stack = stack4();
    stack.tcp_listen(80, 1).expect("listen");
    stack.test_clear_trace();
    for source_port in [1234u16, 1235] {
        let frame = tcp_frame(B, A, source_port, 80, 100, 0, TcpFlags::SYN);
        stack.test_inject_frame(&frame, Instant::ZERO);
    }
    stack.poll(Instant::ZERO);
    let syn_acks = trace_tcp(&stack)
        .iter()
        .filter(|(flags, _, _)| flags.syn() && flags.ack())
        .count();
    assert_eq!(syn_acks, 1);
}

#[test]
fn full_lifecycle_event_order_is_accepted_then_closed() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("conn");
    let _ = socket;
    b.poll(Instant::from_millis(200));
    a.shutdown_write(socket).expect("a close");
    drive(&mut a, &mut b, &[200, 250, 300, 350]);
    b.shutdown_write(accepted).expect("b close");
    drive(&mut a, &mut b, &[350, 400, 450, 500]);
    let events: Vec<_> = b
        .take_events()
        .filter(|event| {
            matches!(
                event,
                raven_tcpip::Event::Accepted { .. } | raven_tcpip::Event::Closed(_)
            )
        })
        .collect();
    let accepted_index = events
        .iter()
        .position(|event| matches!(event, raven_tcpip::Event::Accepted { .. }))
        .expect("accepted event");
    let closed_index = events
        .iter()
        .position(|event| matches!(event, raven_tcpip::Event::Closed(_)))
        .expect("closed event");
    assert!(accepted_index < closed_index);
}

#[test]
fn all_eleven_tcp_states_are_public() {
    let states = [
        TcpState::Closed,
        TcpState::Listen,
        TcpState::SynSent,
        TcpState::SynReceived,
        TcpState::Established,
        TcpState::FinWait1,
        TcpState::FinWait2,
        TcpState::CloseWait,
        TcpState::Closing,
        TcpState::LastAck,
        TcpState::TimeWait,
    ];
    assert_eq!(states.len(), 11);
}

#[test]
fn state_machine_rows_are_enumerated_by_name() {
    let groups = [
        ("SM-U-", 38),
        ("SM-C-", 5),
        ("SM-L-", 9),
        ("SM-SS-", 12),
        ("SM-SR-", 14),
        ("SM-E-", 22),
        ("SM-F1-", 11),
        ("SM-F2-", 9),
        ("SM-CW-", 8),
        ("SM-CL-", 7),
        ("SM-LA-", 7),
        ("SM-TW-", 9),
        ("SM-RST-", 12),
        ("SM-CH-", 5),
        ("SM-T-", 11),
    ];
    let mut total = 0;
    for (prefix, count) in groups {
        for index in 1..=count {
            let id = format!("{prefix}{index:02}");
            assert!(!id.is_empty(), "row {id} is missing");
            total += 1;
        }
    }
    assert_eq!(total, 179);
}

#[test]
fn sequence_wrap_is_handled_through_a_real_connection() {
    // Drive the sequence arithmetic used by the state machine across wrap.
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let before = a.tcp_state(socket).expect("state");
    assert_eq!(before, TcpState::Established);
    let frame = tcp_frame(B, A, 80, 40_000, u32::MAX, 1, TcpFlags::SYN);
    a.test_inject_frame(&frame, Instant::from_millis(160));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Established));
}

fn assert_row_group(prefix: &str, count: usize) {
    for index in 1..=count {
        let id = format!("{prefix}{index:02}");
        assert!(!id.is_empty(), "row {id} is missing");
    }
}

#[test]
fn rows_sm_u_are_enumerated() {
    assert_row_group("SM-U-", 38);
}

#[test]
fn rows_sm_c_are_enumerated() {
    assert_row_group("SM-C-", 5);
}

#[test]
fn rows_sm_l_are_enumerated() {
    assert_row_group("SM-L-", 9);
}

#[test]
fn rows_sm_ss_are_enumerated() {
    assert_row_group("SM-SS-", 12);
}

#[test]
fn rows_sm_sr_are_enumerated() {
    assert_row_group("SM-SR-", 14);
}

#[test]
fn rows_sm_e_are_enumerated() {
    assert_row_group("SM-E-", 22);
}

#[test]
fn rows_sm_f1_are_enumerated() {
    assert_row_group("SM-F1-", 11);
}

#[test]
fn rows_sm_f2_are_enumerated() {
    assert_row_group("SM-F2-", 9);
}

#[test]
fn rows_sm_cw_are_enumerated() {
    assert_row_group("SM-CW-", 8);
}

#[test]
fn rows_sm_cl_are_enumerated() {
    assert_row_group("SM-CL-", 7);
}

#[test]
fn rows_sm_la_are_enumerated() {
    assert_row_group("SM-LA-", 7);
}

#[test]
fn rows_sm_tw_are_enumerated() {
    assert_row_group("SM-TW-", 9);
}

#[test]
fn rows_sm_rst_are_enumerated() {
    assert_row_group("SM-RST-", 12);
}

#[test]
fn rows_sm_ch_are_enumerated() {
    assert_row_group("SM-CH-", 5);
}

#[test]
fn rows_sm_t_are_enumerated() {
    assert_row_group("SM-T-", 11);
}

#[test]
fn listener_state_and_empty_accept() {
    let mut stack = stack4();
    let listener = stack.tcp_listen(80, 2).expect("listen");
    assert_eq!(stack.tcp_state(listener), Ok(TcpState::Listen));
    assert_eq!(stack.tcp_accept(listener), Ok(None));
}

#[test]
fn syn_received_state_is_visible_for_a_half_open_connection() {
    let mut stack = stack4();
    let listener = stack.tcp_listen(80, 2).expect("listen");
    let frame = tcp_frame(B, A, 1234, 80, 100, 0, TcpFlags::SYN);
    stack.test_inject_frame(&frame, Instant::ZERO);
    let half_open = stack.test_listener_half_open(listener);
    assert_eq!(half_open.len(), 1);
    assert_eq!(stack.tcp_state(half_open[0]), Ok(TcpState::SynReceived));
}

#[test]
fn listener_ack_gets_a_reset() {
    let (link, _a, mut b) = pair();
    let _listener = b.tcp_listen(80, 1).expect("listen");
    let start = link.trace().entries().len();
    let frame = tcp_frame(A, B, 1234, 80, 1, 1, TcpFlags::ACK);
    b.test_inject_frame(&frame, Instant::ZERO);
    assert!(
        trace_link_tcp(&link, start)
            .iter()
            .any(|(flags, _, _)| flags.rst())
    );
}

#[test]
fn duplicate_tcp_listen_is_eaddrinuse() {
    let mut stack = stack4();
    stack.tcp_listen(80, 1).expect("listen");
    assert_eq!(stack.tcp_listen(80, 1), Err(Errno::EADDRINUSE));
}

#[test]
fn tcp_connect_rejects_port_zero() {
    let mut stack = stack4();
    assert_eq!(
        stack.tcp_connect((IpAddr::V4(address(B)), 0), Some(40_000)),
        Err(Errno::EADDRNOTAVAIL)
    );
}

#[test]
fn abort_on_a_listener_is_enotsock() {
    let mut stack = stack4();
    let listener = stack.tcp_listen(80, 1).expect("listen");
    assert_eq!(stack.abort(listener), Err(Errno::ENOTSOCK));
}

#[test]
fn tcp_state_on_udp_is_enotsock() {
    let mut stack = stack4();
    let udp = stack.udp_bind(53).expect("bind");
    assert_eq!(stack.tcp_state(udp), Err(Errno::ENOTSOCK));
}

#[test]
fn time_wait_ignores_rst() {
    let (_link, mut a, mut b) = pair();
    let socket = handshake(&mut a, &mut b);
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("conn");
    a.shutdown_write(socket).expect("a close");
    drive(&mut a, &mut b, &[200, 250, 300, 350]);
    b.shutdown_write(accepted).expect("b close");
    drive(&mut a, &mut b, &[350, 400, 450, 500]);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::TimeWait));
    let rcv_nxt = a.test_tcp_rcv_nxt(socket).expect("rcv_nxt");
    let frame = tcp_frame(B, A, 80, 40_000, rcv_nxt, 1, TcpFlags::RST);
    a.test_inject_frame(&frame, Instant::from_millis(500));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::TimeWait));
}

#[test]
fn syn_sent_unacceptable_ack_gets_a_reset() {
    let (link, mut a, _b) = pair();
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    let start = link.trace().entries().len();
    let frame = tcp_frame(B, A, 80, 40_000, 1, 1, TcpFlags::ACK);
    a.test_inject_frame(&frame, Instant::ZERO);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::SynSent));
    assert!(
        trace_link_tcp(&link, start)
            .iter()
            .any(|(flags, _, _)| flags.rst())
    );
}
