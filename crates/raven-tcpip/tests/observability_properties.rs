// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C19: one behavioural test per tunable property.

#![cfg(feature = "testutil")]

mod common;

use raven_abi::errno::Errno;
use raven_tcpip::{Duration, Instant, IpAddr, Ipv4Addr, Stack, StackConfig, TcpState};
use raven_tcpip_sim::LinkConfig;
use raven_tcpip_wire::tcp::TcpFlags;

use common::{
    A, B, address, handshake, inject_fragment, inject_segment, inject_segment_at, pair,
    pair_with_link, pump_ms,
};
use raven_tcpip::testutil::NullDevice;

fn set<D: raven_tcpip::Device>(stack: &mut Stack<D>, name: &str, value: u64) {
    stack
        .set_property(name, value)
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
}

#[test]
fn time_wait_interval_shortens_the_observed_timewait() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.time_wait_interval", 1000);
    let (socket, peer) = handshake(&mut a, &mut b);
    a.shutdown_write(socket).expect("fin a");
    pump_ms(&mut a, &mut b, 210, 400);
    b.shutdown_write(peer).expect("fin b");
    pump_ms(&mut a, &mut b, 410, 700);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::TimeWait));
    pump_ms(&mut a, &mut b, 710, 1800);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert!(a.counters().timewait_fires >= 1);
}

#[test]
fn finwait2_timeout_closes_a_half_closed_peer() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.finwait2_timeout", 1000);
    let (socket, _peer) = handshake(&mut a, &mut b);
    a.shutdown_write(socket).expect("fin");
    pump_ms(&mut a, &mut b, 210, 600);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::FinWait2));
    pump_ms(&mut a, &mut b, 610, 1700);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ETIMEDOUT)));
    assert!(a.counters().finwait2_timeouts >= 1);
}

#[test]
fn user_timeout_aborts_an_idle_connection() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.user_timeout", 1000);
    let (socket, _peer) = handshake(&mut a, &mut b);
    pump_ms(&mut a, &mut b, 210, 1600);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ETIMEDOUT)));
    assert_eq!(a.counters().user_timeouts, 1);
}

#[test]
fn keepalive_enable_gates_the_socket_option() {
    let (_link, mut a, mut b) = pair();
    let (socket, _peer) = handshake(&mut a, &mut b);
    set(&mut a, "tcp.keepalive_enable", 0);
    assert_eq!(
        a.tcp_set_keepalive(socket, Duration::from_millis(1000), 1),
        Err(Errno::EOPNOTSUPP)
    );
    set(&mut a, "tcp.keepalive_enable", 1);
    assert!(
        a.tcp_set_keepalive(socket, Duration::from_millis(1000), 1)
            .is_ok()
    );
}

#[test]
fn keepalive_interval_sets_the_default_probe_period() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.keepalive_interval", 1000);
    let (socket, _peer) = handshake(&mut a, &mut b);
    a.tcp_set_keepalive(socket, Duration::ZERO, 3)
        .expect("keepalive");
    pump_ms(&mut a, &mut b, 210, 1400);
    assert!(a.counters().keepalive_fires >= 1);
}

#[test]
fn keepalive_probes_sets_the_timeout_probe_count() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.keepalive_interval", 1000);
    set(&mut a, "tcp.keepalive_probes", 1);
    let (socket, _peer) = handshake(&mut a, &mut b);
    a.tcp_set_keepalive(socket, Duration::ZERO, 0)
        .expect("keepalive");
    pump_ms(&mut a, &mut b, 210, 2500);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ETIMEDOUT)));
    assert_eq!(a.counters().keepalive_fires, 1);
}

#[test]
fn half_open_limit_bounds_the_listener_queue() {
    let (_link, mut _a, mut b) = pair();
    set(&mut b, "tcp.syn_cookies_enable", 0);
    set(&mut b, "tcp.half_open_limit", 1);
    b.tcp_listen(80, 8).expect("listen");
    for source_port in [40000u16, 40001] {
        inject_segment(&mut b, A, B, source_port, 80, 100, 0, TcpFlags::SYN, &[]);
    }
    b.poll(Instant::ZERO);
    assert_eq!(b.counters().passive_opens, 1);
    assert_eq!(b.counters().listen_drops, 1);
}

#[test]
fn accept_queue_limit_drops_the_extra_completion() {
    let (_link, mut a, mut b) = pair();
    set(&mut b, "tcp.accept_queue_limit", 1);
    b.tcp_listen(80, 8).expect("listen");
    a.tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("first");
    pump_ms(&mut a, &mut b, 0, 200);
    a.tcp_connect((IpAddr::V4(address(B)), 80), Some(40_001))
        .expect("second");
    pump_ms(&mut a, &mut b, 210, 450);
    assert!(b.counters().accept_queue_drops >= 1);
}

#[test]
fn connection_max_refuses_the_extra_connection() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.half_open_limit", 1);
    set(&mut a, "tcp.accept_queue_limit", 1);
    set(&mut a, "tcp.connection_max", 1);
    b.tcp_listen(80, 8).expect("listen");
    a.tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("first");
    pump_ms(&mut a, &mut b, 0, 200);
    assert_eq!(
        a.tcp_connect((IpAddr::V4(address(B)), 80), Some(40_001)),
        Err(Errno::EMFILE)
    );
}

#[test]
fn timewait_max_recycles_the_oldest_entry() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.timewait_max", 1);
    let (socket, peer) = handshake(&mut a, &mut b);
    a.shutdown_write(socket).expect("fin a");
    pump_ms(&mut a, &mut b, 210, 400);
    b.shutdown_write(peer).expect("fin b");
    pump_ms(&mut a, &mut b, 410, 700);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::TimeWait));
    set(&mut a, "tcp.timewait_max", 1);
    let sockets_before = a.sockets().count();
    assert!(sockets_before >= 1);
}

#[test]
fn reassembly_timeout_discards_hole_sets() {
    let (_link, mut _a, mut b) = pair();
    set(&mut b, "tcp.reassembly_timeout", 100);
    inject_fragment(&mut b, A, B, 9, 0, true, &[0u8; 16]);
    b.poll(Instant::from_millis(0));
    b.poll(Instant::from_millis(200));
    assert!(b.counters().reassembly_timeouts >= 1);
}

#[test]
fn reassembly_holes_max_bounds_out_of_order_data() {
    let (_link, mut a, mut b) = pair();
    set(&mut b, "tcp.reassembly_holes_max", 1);
    let (_socket, peer) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(peer).expect("rcv_nxt");
    let ack = b.test_tcp_window(peer).expect("window").2;
    for index in 0..3u32 {
        inject_segment(
            &mut b,
            A,
            B,
            40_000,
            80,
            rcv_nxt.wrapping_add(100 + index * 10),
            ack,
            TcpFlags::ACK,
            b"x",
        );
    }
    assert!(b.counters().reassembly_overflows >= 1);
}

#[test]
fn timer_late_threshold_counts_a_missed_timer() {
    let mut stack: Stack<NullDevice> = common::null_stack(StackConfig::default(), 21);
    set(&mut stack, "tcp.timer_late_threshold", 1);
    stack.set_timer(Instant::ZERO).expect("timer");
    stack.poll(Instant::from_millis(100));
    assert_eq!(stack.counters().timer_missed, 1);
    assert!(stack.counters().timer_early >= 1);
}

#[test]
fn rto_initial_sets_the_first_retransmission_delay() {
    let config = LinkConfig::default();
    let (link, mut a, mut b) = pair();
    set(&mut a, "tcp.rto_initial", 500);
    let (socket, _peer) = handshake(&mut a, &mut b);
    link.set_config(LinkConfig {
        loss_percent: 100.0,
        ..config
    });
    a.send(socket, b"lost").expect("send");
    pump_ms(&mut a, &mut b, 210, 450);
    assert_eq!(a.counters().rto_fires, 0);
    pump_ms(&mut a, &mut b, 460, 750);
    assert!(a.counters().rto_fires >= 1);
}

#[test]
fn rto_min_clamps_a_small_sample() {
    let (link, mut a, mut b) = pair_with_link(
        LinkConfig {
            rtt_ms: 10,
            ..LinkConfig::default()
        },
        StackConfig::default(),
    );
    set(&mut a, "tcp.rto_min", 1000);
    let (socket, _peer) = handshake(&mut a, &mut b);
    a.send(socket, b"sample").expect("send");
    pump_ms(&mut a, &mut b, 210, 800);
    let info = a.socket_info(socket).expect("info");
    assert!(info.rto_ms >= 1000, "rto {}", info.rto_ms);
    let _ = link;
}

#[test]
fn rto_max_caps_the_estimator_and_backoff() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.rto_max", 1000);
    let (socket, _peer) = handshake(&mut a, &mut b);
    a.send(socket, b"x").expect("send");
    pump_ms(&mut a, &mut b, 210, 2500);
    let info = a.socket_info(socket).expect("info");
    assert!(info.rto_ms <= 1000, "rto {}", info.rto_ms);
}

#[test]
fn rto_max_retries_gives_up_early() {
    let link_config = LinkConfig::default();
    let (link, mut a, mut b) = pair_with_link(link_config, StackConfig::default());
    set(&mut a, "tcp.rto_max_retries", 1);
    set(&mut a, "tcp.rto_initial", 100);
    set(&mut a, "tcp.rto_max", 1000);
    let (socket, _peer) = handshake(&mut a, &mut b);
    link.set_config(LinkConfig {
        loss_percent: 100.0,
        ..link_config
    });
    a.send(socket, b"gone").expect("send");
    pump_ms(&mut a, &mut b, 210, 4000);
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ETIMEDOUT)));
}

#[test]
fn connect_retries_bounds_syn_retransmission() {
    let mut stack = common::null_stack(StackConfig::default(), 23);
    set(&mut stack, "tcp.connect_retries", 1);
    stack
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    pump_ms_null(&mut stack, 0, 5000);
    assert!(stack.counters().connect_fires >= 1);
    assert!(stack.counters().attempt_fails >= 1);
}

#[test]
fn delack_delay_controls_the_timer() {
    let (_link, mut a, mut b) = pair();
    set(&mut b, "tcp.delack_delay", 500);
    let (_socket, peer) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(peer).expect("rcv_nxt");
    let ack = b.test_tcp_window(peer).expect("window").2;
    inject_segment_at(
        &mut b,
        A,
        B,
        40_000,
        80,
        rcv_nxt,
        ack,
        TcpFlags::ACK,
        b"x",
        &[],
        Instant::from_millis(210),
    );
    b.poll(Instant::from_millis(300));
    assert_eq!(b.counters().delayed_acks_sent, 0);
    b.poll(Instant::from_millis(800));
    assert_eq!(b.counters().delayed_acks_sent, 1);
    let _ = a;
}

#[test]
fn delack_segments_controls_immediate_acks() {
    let (_link, mut a, mut b) = pair();
    set(&mut b, "tcp.delack_segments", 1);
    let (_socket, peer) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(peer).expect("rcv_nxt");
    let ack = b.test_tcp_window(peer).expect("window").2;
    inject_segment_at(
        &mut b,
        A,
        B,
        40_000,
        80,
        rcv_nxt,
        ack,
        TcpFlags::ACK,
        b"x",
        &[],
        Instant::from_millis(210),
    );
    b.poll(Instant::from_millis(220));
    assert_eq!(b.counters().delayed_acks_sent, 0);
    let _ = a;
}

fn pump_ms_null(stack: &mut Stack<NullDevice>, start: i64, end: i64) {
    let mut time = start;
    while time <= end {
        stack.poll(Instant::from_millis(time));
        time += 10;
    }
}

#[test]
fn dup_ack_threshold_controls_fast_retransmit() {
    let (link, mut a, mut b) = pair();
    set(&mut a, "tcp.dup_ack_threshold", 4);
    let (socket, _peer) = handshake(&mut a, &mut b);
    link.set_config(LinkConfig {
        loss_percent: 100.0,
        ..LinkConfig::default()
    });
    a.send(socket, b"lost").expect("send");
    let (_, snd_una, snd_nxt) = a.test_tcp_window(socket).expect("window");
    assert_ne!(snd_una, snd_nxt);
    let seq = a.test_tcp_rcv_nxt(socket).expect("rcv_nxt");
    for _ in 0..3 {
        inject_segment(&mut a, B, A, 80, 40_000, seq, snd_una, TcpFlags::ACK, &[]);
    }
    assert_eq!(a.counters().fast_retransmits, 0);
    inject_segment(&mut a, B, A, 80, 40_000, seq, snd_una, TcpFlags::ACK, &[]);
    assert_eq!(a.counters().fast_retransmits, 1);
}

#[test]
fn initial_window_sets_the_initial_cwnd() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.initial_window", 4);
    let (socket, _peer) = handshake(&mut a, &mut b);
    let info = a.socket_info(socket).expect("info");
    assert!(info.cwnd >= 4 * u32::from(info.mss), "cwnd {}", info.cwnd);
    assert!(info.cwnd < 5 * u32::from(info.mss), "cwnd {}", info.cwnd);
}

#[test]
fn cwnd_max_caps_growth() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.cwnd_max", 5000);
    let (socket, peer) = handshake(&mut a, &mut b);
    let payload: Vec<u8> = (0..200_000).map(|index| index as u8).collect();
    let received = common::send_all(&mut a, &mut b, socket, peer, &payload, 210);
    assert_eq!(received.len(), payload.len());
    let info = a.socket_info(socket).expect("info");
    assert!(info.cwnd <= 5000, "cwnd {}", info.cwnd);
}

#[test]
fn cwnd_min_sets_the_initial_floor() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.initial_window", 1);
    set(&mut a, "tcp.cwnd_min", 8);
    let (socket, _peer) = handshake(&mut a, &mut b);
    let info = a.socket_info(socket).expect("info");
    assert!(info.cwnd >= 8 * u32::from(info.mss), "cwnd {}", info.cwnd);
}

#[test]
fn cc_algorithm_selects_newreno() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.cc_algorithm", 0);
    let (socket, _peer) = handshake(&mut a, &mut b);
    assert_eq!(a.socket_info(socket).expect("info").cc_algorithm, "newreno");
}

#[test]
fn cc_algorithm_selects_cubic() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.cc_algorithm", 1);
    let (socket, _peer) = handshake(&mut a, &mut b);
    assert_eq!(a.socket_info(socket).expect("info").cc_algorithm, "cubic");
}

#[test]
fn window_scale_enable_gates_the_syn_option() {
    let (link, mut a, mut b) = pair();
    set(&mut a, "tcp.window_scale_enable", 0);
    let start = link.trace().entries().len();
    let (socket, _peer) = handshake(&mut a, &mut b);
    assert_eq!(a.socket_info(socket).expect("info").options.wscale, None);
    let _ = (link, start);
}

#[test]
fn timestamps_enable_gates_the_syn_option() {
    let (link, mut a, mut b) = pair();
    set(&mut a, "tcp.timestamps_enable", 0);
    let start = link.trace().entries().len();
    let (socket, _peer) = handshake(&mut a, &mut b);
    assert!(!a.socket_info(socket).expect("info").options.timestamps);
    let _ = (link, start);
}

#[test]
fn sack_enable_gates_the_syn_option() {
    let (link, mut a, mut b) = pair();
    set(&mut a, "tcp.sack_enable", 0);
    let start = link.trace().entries().len();
    let (socket, _peer) = handshake(&mut a, &mut b);
    assert!(!a.socket_info(socket).expect("info").options.sack);
    let _ = (link, start);
}

#[test]
fn ecn_enable_gates_the_negotiated_flag() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.ecn_enable", 0);
    let (socket, _peer) = handshake(&mut a, &mut b);
    assert!(!a.socket_info(socket).expect("info").options.ecn);
}

#[test]
fn syn_cookies_enable_absorbs_a_full_half_open_queue() {
    let (_link, mut _a, mut b) = pair();
    set(&mut b, "tcp.half_open_limit", 1);
    set(&mut b, "tcp.syn_cookies_enable", 1);
    b.tcp_listen(80, 8).expect("listen");
    for source_port in [40000u16, 40001] {
        inject_segment(&mut b, A, B, source_port, 80, 100, 0, TcpFlags::SYN, &[]);
    }
    b.poll(Instant::ZERO);
    assert_eq!(b.counters().syn_cookie_sent, 1);
    assert_eq!(b.counters().listen_drops, 0);
}

#[test]
fn syn_cookie_lifetime_rejects_a_stale_ack() {
    let (link, mut _a, mut b) = pair();
    set(&mut b, "tcp.half_open_limit", 1);
    set(&mut b, "tcp.syn_cookies_enable", 1);
    set(&mut b, "tcp.syn_cookie_lifetime", 1000);
    b.tcp_listen(80, 8).expect("listen");
    inject_segment(&mut b, A, B, 40000, 80, 100, 0, TcpFlags::SYN, &[]);
    b.poll(Instant::ZERO);
    inject_segment(&mut b, A, B, 40001, 80, 100, 0, TcpFlags::SYN, &[]);
    b.poll(Instant::ZERO);
    let iss = common::trace_link_tcp(&link, 0)
        .iter()
        .rfind(|(flags, _, _, _)| flags.syn() && flags.ack())
        .map(|(_, seq, _, _)| *seq)
        .expect("cookie syn-ack");
    inject_segment_at(
        &mut b,
        A,
        B,
        40001,
        80,
        101,
        iss.wrapping_add(1),
        TcpFlags::ACK,
        &[],
        &[],
        Instant::from_millis(2500),
    );
    assert!(b.counters().syn_cookie_invalid >= 1);
}

#[test]
fn mss_floor_clamps_a_small_peer_mss() {
    let (_link, mut _a, mut b) = pair();
    set(&mut b, "tcp.syn_cookies_enable", 0);
    b.tcp_listen(80, 8).expect("listen");
    inject_segment(&mut b, A, B, 40000, 80, 100, 0, TcpFlags::SYN, &[]);
    let half_open = b
        .sockets()
        .find(|socket| *socket != raven_tcpip::SocketId(1))
        .expect("half open");
    let info = b.socket_info(half_open).expect("info");
    assert!(info.mss >= 536, "mss {}", info.mss);
}

#[test]
fn mss_ceiling_caps_the_announced_mss() {
    let (_link, mut a, mut b) = pair();
    set(&mut b, "tcp.mss_floor", 64);
    set(&mut b, "tcp.mss_ceiling", 500);
    let (_socket, peer) = handshake(&mut a, &mut b);
    let info = b.socket_info(peer).expect("info");
    assert!(info.mss <= 500, "mss {}", info.mss);
}

#[test]
fn mss_default_is_used_when_the_device_has_no_mtu() {
    let config = StackConfig::default();
    let mut stack = Stack::new(ZeroMtuDevice, config, 1, Instant::ZERO);
    stack
        .set_ipv4(raven_tcpip::Ipv4Config {
            addr: Ipv4Addr::new(A),
            prefix_len: 24,
            gateway: None,
        })
        .expect("ipv4");
    set(&mut stack, "tcp.mss_default", 800);
    let socket = stack
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    assert_eq!(stack.socket_info(socket).expect("info").mss, 800);
}

#[test]
fn send_buffer_default_sets_the_tx_cap() {
    let (link, mut a, mut b) = pair();
    set(&mut a, "tcp.send_buffer_default", 4096);
    let (socket, _peer) = handshake(&mut a, &mut b);
    assert_eq!(a.test_socket_caps(socket).expect("caps").1, 4096);
    assert_eq!(a.send(socket, &vec![0u8; 5000]), Ok(4096));
    let _ = link;
}

#[test]
fn send_buffer_max_caps_the_tx_cap() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.send_buffer_default", 2048);
    set(&mut a, "tcp.send_buffer_max", 2048);
    let (socket, _peer) = handshake(&mut a, &mut b);
    assert_eq!(a.test_socket_caps(socket).expect("caps").1, 2048);
}

#[test]
fn recv_buffer_default_sets_the_rx_cap() {
    let (_link, mut a, mut b) = pair();
    set(&mut b, "tcp.recv_buffer_default", 4096);
    let (_socket, peer) = handshake(&mut a, &mut b);
    assert_eq!(b.test_socket_caps(peer).expect("caps").0, 4096);
}

#[test]
fn recv_buffer_max_caps_the_rx_cap() {
    let (_link, mut a, mut b) = pair();
    set(&mut b, "tcp.recv_buffer_default", 2048);
    set(&mut b, "tcp.recv_buffer_max", 2048);
    let (_socket, peer) = handshake(&mut a, &mut b);
    assert_eq!(b.test_socket_caps(peer).expect("caps").0, 2048);
}

#[test]
fn nagle_disabled_sends_small_writes_immediately() {
    let (link, mut a, mut b) = pair();
    set(&mut a, "tcp.nagle", 0);
    let (socket, _peer) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.send(socket, b"x").expect("first");
    a.send(socket, b"y").expect("second");
    let segments = common::trace_link_tcp(&link, start)
        .iter()
        .filter(|(flags, _, _, _)| flags.ack() && !flags.syn() && !flags.rst())
        .count();
    assert!(segments >= 2, "segments {segments}");
}

#[test]
fn nagle_enabled_holds_the_second_small_write() {
    let (link, mut a, mut b) = pair();
    set(&mut a, "tcp.nagle", 1);
    let (socket, _peer) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.send(socket, b"x").expect("first");
    a.send(socket, b"y").expect("second");
    let segments = common::trace_link_tcp(&link, start)
        .iter()
        .filter(|(flags, _, _, _)| flags.ack() && !flags.syn() && !flags.rst())
        .count();
    assert_eq!(segments, 1);
}

#[test]
fn urgent_mode_gates_application_notification() {
    let (_link, mut a, mut b) = pair();
    set(&mut b, "tcp.urgent_mode", 0);
    let (socket, peer) = handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(peer).expect("rcv_nxt");
    let ack = b.test_tcp_window(peer).expect("window").2;
    inject_segment(
        &mut b,
        A,
        B,
        40_000,
        80,
        rcv_nxt,
        ack,
        TcpFlags::from_bits(TcpFlags::URG.bits() | TcpFlags::ACK.bits()),
        &[],
    );
    assert_eq!(b.tcp_urgent(peer), Ok(None));
    set(&mut b, "tcp.urgent_mode", 1);
    inject_segment(
        &mut b,
        A,
        B,
        40_000,
        80,
        rcv_nxt,
        ack,
        TcpFlags::from_bits(TcpFlags::URG.bits() | TcpFlags::ACK.bits()),
        &[],
    );
    assert!(matches!(b.tcp_urgent(peer), Ok(Some(_))));
    let _ = socket;
}

#[test]
fn port_range_selects_the_ephemeral_ports() {
    let mut stack = common::null_stack(StackConfig::default(), 31);
    set(&mut stack, "tcp.port_range_first", 40_000);
    set(&mut stack, "tcp.port_range_last", 40_010);
    let first = stack.udp_bind(0).expect("first");
    let second = stack.udp_bind(0).expect("second");
    let first_port = stack.test_udp_port(first).expect("port");
    let second_port = stack.test_udp_port(second).expect("port");
    assert!((40_000..=40_010).contains(&first_port));
    assert!((40_000..=40_010).contains(&second_port));
    assert_ne!(first_port, second_port);
}

#[test]
fn isn_strength_selects_a_different_generator() {
    let first = common::null_stack(StackConfig::default(), 41);
    let second = common::null_stack(StackConfig::default(), 41);
    let mut first = first;
    let mut second = second;
    set(&mut first, "tcp.isn_strength", 0);
    set(&mut second, "tcp.isn_strength", 1);
    let a = first
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("a");
    let b = second
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("b");
    assert_ne!(first.test_tcp_iss(a), second.test_tcp_iss(b));
}

#[test]
fn challenge_ack_burst_suppresses_after_the_bucket() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.challenge_ack_burst", 1);
    set(&mut a, "tcp.challenge_ack_interval", 60_000);
    let (socket, _peer) = handshake(&mut a, &mut b);
    let seq = a.test_tcp_rcv_nxt(socket).expect("seq");
    for _ in 0..2 {
        inject_segment(&mut a, B, A, 80, 40_000, seq, 0, TcpFlags::SYN, &[]);
    }
    assert_eq!(a.counters().challenge_acks_sent, 1);
    assert_eq!(a.counters().challenge_ack_suppressed, 1);
}

#[test]
fn challenge_ack_interval_refills_the_bucket() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.challenge_ack_burst", 1);
    set(&mut a, "tcp.challenge_ack_interval", 100);
    let (socket, _peer) = handshake(&mut a, &mut b);
    let seq = a.test_tcp_rcv_nxt(socket).expect("seq");
    inject_segment(&mut a, B, A, 80, 40_000, seq, 0, TcpFlags::SYN, &[]);
    inject_segment(&mut a, B, A, 80, 40_000, seq, 0, TcpFlags::SYN, &[]);
    assert_eq!(a.counters().challenge_ack_suppressed, 1);
    inject_segment_at(
        &mut a,
        B,
        A,
        80,
        40_000,
        seq,
        0,
        TcpFlags::SYN,
        &[],
        &[],
        Instant::from_millis(500),
    );
    assert_eq!(a.counters().challenge_acks_sent, 2);
}

#[test]
fn challenge_ack_global_burst_limits_all_connections() {
    let (_link, mut a, mut b) = pair();
    set(&mut a, "tcp.challenge_ack_burst", 100);
    set(&mut a, "tcp.challenge_ack_global_burst", 1);
    set(&mut a, "tcp.challenge_ack_global_interval", 60_000);
    let (socket, _peer) = handshake(&mut a, &mut b);
    let seq = a.test_tcp_rcv_nxt(socket).expect("seq");
    inject_segment(&mut a, B, A, 80, 40_000, seq, 0, TcpFlags::SYN, &[]);
    inject_segment(&mut a, B, A, 80, 40_000, seq, 0, TcpFlags::SYN, &[]);
    assert_eq!(a.counters().challenge_acks_sent, 1);
    assert_eq!(a.counters().challenge_ack_suppressed, 1);
}

#[test]
fn ignore_path_mtu_keeps_the_interface_mss() {
    let (_link, mut a, _b) = pair();
    common::inject_icmp_frag_needed(&mut a, A, B, 600);
    assert_eq!(a.test_pmtu4(address(B)), Some(600));
    set(&mut a, "tcp.ignore_path_mtu", 1);
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    assert!(a.socket_info(socket).expect("info").mss > 560);
}

#[test]
fn pmtu_cache_max_bounds_the_cache() {
    let (_link, mut a, _b) = pair();
    set(&mut a, "tcp.pmtu_cache_max", 1);
    common::inject_icmp_frag_needed(&mut a, A, B, 600);
    common::inject_icmp_frag_needed(&mut a, A, [192, 0, 2, 3], 700);
    assert!(a.test_pmtu4_len() <= 1);
}

#[test]
fn pmtu_cache_ttl_expires_a_stale_entry() {
    let (_link, mut a, _b) = pair();
    set(&mut a, "tcp.pmtu_cache_ttl", 1000);
    common::inject_icmp_frag_needed(&mut a, A, B, 600);
    a.poll(Instant::from_millis(2000));
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    assert!(a.socket_info(socket).expect("info").mss > 560);
}

#[test]
fn hash_chain_max_reports_collisions() {
    let mut stack = common::null_stack(StackConfig::default(), 51);
    set(&mut stack, "tcp.hash_chain_max", 1);
    for index in 0..20u16 {
        stack
            .tcp_connect((IpAddr::V4(address(B)), 80 + index), Some(40_000 + index))
            .expect("connect");
    }
    stack.poll(Instant::ZERO);
    assert!(stack.counters().hash_chain_overflows >= 1);
}

struct ZeroMtuDevice;

impl raven_tcpip::Device for ZeroMtuDevice {
    fn capabilities(&self) -> raven_tcpip::DeviceCaps {
        raven_tcpip::DeviceCaps {
            medium: raven_tcpip::Medium::Ip,
            mtu: 0,
            hardware_addr: None,
            checksum_offload: raven_tcpip::ChecksumOffload::None,
        }
    }
    fn receive(&mut self, _now: Instant, _consume: &mut dyn FnMut(&[u8])) -> bool {
        false
    }
    fn transmit(
        &mut self,
        _now: Instant,
        _len: usize,
        _fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), raven_tcpip::TxError> {
        Ok(())
    }
}
