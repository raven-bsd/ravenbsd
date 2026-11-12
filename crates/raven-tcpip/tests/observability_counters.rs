// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C19: counters move on the events the specification names.

#![cfg(feature = "testutil")]

mod common;

use raven_tcpip::{Instant, IpAddr, StackConfig, TcpState};
use raven_tcpip_sim::LinkConfig;
use raven_tcpip_wire::tcp::TcpFlags;

use common::{A, B, address, handshake, inject_segment, pair, pair_with_link, pump_ms};

#[test]
fn handshake_moves_open_and_segment_counters() {
    let (_link, mut a, mut b) = pair();
    let (_socket, _peer) = handshake(&mut a, &mut b);
    assert_eq!(a.counters().active_opens, 1);
    assert_eq!(b.counters().passive_opens, 1);
    assert!(a.counters().out_segments >= 2);
    assert!(b.counters().in_segments >= 1);
}

#[test]
fn transfer_moves_byte_and_ack_counters() {
    let (_link, mut a, mut b) = pair();
    let (socket, peer) = handshake(&mut a, &mut b);
    a.send(socket, b"0123456789").expect("send");
    pump_ms(&mut a, &mut b, 210, 600);
    let mut buffer = [0u8; 16];
    assert_eq!(b.recv(peer, &mut buffer).expect("recv"), 10);
    assert_eq!(a.counters().out_bytes, 10);
    assert_eq!(b.counters().in_bytes, 10);
    assert!(a.counters().out_segments >= 3);
    assert!(b.counters().in_segments >= 3);
}

#[test]
fn duplicate_data_moves_the_dup_counters() {
    let (_link, mut a, mut b) = pair();
    let (_socket, peer) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(peer).expect("rcv_nxt");
    let ack = b.test_tcp_window(peer).expect("window").2;
    inject_segment(
        &mut b,
        A,
        B,
        40_000,
        80,
        rcv_nxt.wrapping_sub(1),
        ack,
        TcpFlags::ACK,
        b"d",
    );
    assert_eq!(b.counters().in_dup_segments, 1);
    assert_eq!(b.counters().in_dup_bytes, 1);
}

#[test]
fn out_of_order_data_moves_the_ooo_counters() {
    let (_link, mut a, mut b) = pair();
    let (_socket, peer) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(peer).expect("rcv_nxt");
    let ack = b.test_tcp_window(peer).expect("window").2;
    inject_segment(
        &mut b,
        A,
        B,
        40_000,
        80,
        rcv_nxt.wrapping_add(100),
        ack,
        TcpFlags::ACK,
        b"ooo",
    );
    assert_eq!(b.counters().in_ooo_segments, 1);
    assert_eq!(b.counters().in_ooo_bytes, 3);
}

#[test]
fn data_past_the_window_moves_the_drop_counters() {
    let (_link, mut a, mut b) = pair();
    let (_socket, peer) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(peer).expect("rcv_nxt");
    let ack = b.test_tcp_window(peer).expect("window").2;
    let window = b.test_tcp_window(peer).expect("window").0;
    let beyond = rcv_nxt.wrapping_add(window).wrapping_add(1);
    inject_segment(
        &mut b,
        A,
        B,
        40_000,
        80,
        beyond,
        ack,
        TcpFlags::ACK,
        b"late",
    );
    assert_eq!(b.counters().past_window_drops, 1);
    assert_eq!(b.counters().past_window_bytes, 4);
}

#[test]
fn unknown_options_are_counted() {
    let (_link, mut a, mut b) = pair();
    let (_socket, peer) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(peer).expect("rcv_nxt");
    let ack = b.test_tcp_window(peer).expect("window").2;
    inject_segment(&mut b, A, B, 40_000, 80, rcv_nxt, ack, TcpFlags::ACK, b"");
    common::inject_segment_with_options(
        &mut b,
        A,
        B,
        40_000,
        80,
        rcv_nxt,
        ack,
        TcpFlags::ACK,
        b"",
        &[99, 2],
    );
    assert_eq!(b.counters().option_unknown, 1);
}

#[test]
fn a_valid_reset_moves_the_reset_counters() {
    let (_link, mut a, mut b) = pair();
    let (_socket, peer) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(peer).expect("rcv_nxt");
    inject_segment(&mut b, A, B, 40_000, 80, rcv_nxt, 0, TcpFlags::RST, &[]);
    assert_eq!(b.counters().rst_rcvd, 1);
    assert_eq!(b.counters().estab_resets, 1);
    assert_eq!(b.tcp_state(peer), Ok(TcpState::Closed));
}

#[test]
fn an_ack_above_snd_nxt_moves_acks_for_unsent() {
    let (_link, mut a, mut b) = pair();
    let (socket, _peer) = handshake(&mut a, &mut b);
    let (_, _, snd_nxt) = a.test_tcp_window(socket).expect("window");
    let seq = a.test_tcp_rcv_nxt(socket).expect("seq");
    inject_segment(
        &mut a,
        B,
        A,
        80,
        40_000,
        seq,
        snd_nxt.wrapping_add(1000),
        TcpFlags::ACK,
        &[],
    );
    assert!(a.counters().acks_for_unsent >= 1);
}

#[test]
fn rto_retransmission_moves_the_retransmission_counters() {
    let link_config = LinkConfig::default();
    let (link, mut a, mut b) = pair_with_link(link_config, StackConfig::default());
    let (socket, _peer) = handshake(&mut a, &mut b);
    link.set_config(LinkConfig {
        loss_percent: 100.0,
        ..link_config
    });
    a.send(socket, b"retransmit").expect("send");
    pump_ms(&mut a, &mut b, 210, 2000);
    assert!(a.counters().rto_fires >= 1);
    assert!(a.counters().retrans_segments >= 1);
    assert!(a.counters().retrans_bytes >= 10);
}

#[test]
fn port_allocation_moves_the_probe_counter() {
    let mut stack = common::null_stack(StackConfig::default(), 61);
    let _socket = stack.udp_bind(0).expect("bind");
    assert!(stack.counters().port_alloc_probes >= 1);
}

#[test]
fn syn_cookie_used_counter_moves() {
    let (_link, mut _a, mut b) = pair();
    b.set_property("tcp.half_open_limit", 1).expect("limit");
    b.set_property("tcp.syn_cookies_enable", 1)
        .expect("cookies");
    b.tcp_listen(80, 1).expect("listen");
    for source_port in [40000u16, 40001] {
        inject_segment(&mut b, A, B, source_port, 80, 100, 0, TcpFlags::SYN, &[]);
    }
    assert_eq!(b.counters().syn_cookie_sent, 1);
}

#[test]
fn user_timeout_counter_moves() {
    let (_link, mut a, mut b) = pair();
    a.set_property("tcp.user_timeout", 1000).expect("timeout");
    let (socket, _peer) = handshake(&mut a, &mut b);
    pump_ms(&mut a, &mut b, 210, 1600);
    assert_eq!(a.counters().user_timeouts, 1);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
}

#[test]
fn zero_window_probe_counter_moves() {
    let (_link, mut a, mut b) = pair();
    b.set_property("tcp.recv_buffer_default", 2048)
        .expect("recv buffer");
    let (socket, _peer) = handshake(&mut a, &mut b);
    a.send(socket, &vec![0u8; 8192]).expect("send");
    pump_ms(&mut a, &mut b, 210, 400);
    let (_, snd_una, _) = a.test_tcp_window(socket).expect("window");
    let seq = a.test_tcp_rcv_nxt(socket).expect("seq");
    common::inject_segment_window(
        &mut a,
        B,
        A,
        80,
        40_000,
        seq,
        snd_una,
        TcpFlags::ACK,
        0,
        &[],
    );
    let _ = a.send(socket, b"z");
    pump_ms(&mut a, &mut b, 410, 2400);
    assert!(a.counters().zwprobe_sent >= 1);
    assert_eq!(a.counters().zwprobe_sent, a.counters().zwprobe_fires);
}

#[test]
fn pmtu_update_counter_moves() {
    let (_link, mut a, _b) = pair();
    common::inject_icmp_frag_needed(&mut a, A, B, 600);
    assert_eq!(a.counters().pmtu_updates, 1);
}

#[test]
fn timer_miss_counter_moves() {
    let mut stack = common::null_stack(StackConfig::default(), 62);
    stack
        .set_property("tcp.timer_late_threshold", 1)
        .expect("threshold");
    stack.set_timer(Instant::from_millis(0)).expect("timer");
    stack.poll(Instant::from_millis(100));
    assert_eq!(stack.counters().timer_missed, 1);
}

#[test]
fn active_connection_count_is_visible_through_sockets() {
    let mut stack = common::null_stack(StackConfig::default(), 63);
    stack
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    assert_eq!(stack.sockets().count(), 1);
    assert_eq!(stack.counters().active_opens, 1);
}
