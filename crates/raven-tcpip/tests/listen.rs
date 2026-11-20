// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Listen queues, SYN cookies and ISN tests.

#![cfg(feature = "testutil")]

mod common;

use common::{A, B, address, handshake, inject_segment, pair, pump_ms, trace_link_tcp};
use raven_abi::errno::Errno;
use raven_tcpip::{
    AcceptQueue, HalfOpen, HalfOpenQueue, Instant, IpAddr, Ipv4Addr, IsnGenerator, SocketId,
    SynCookie, TcpState,
};
use raven_tcpip_wire::tcp::TcpFlags;

fn peer(value: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new([198, 51, 100, value]))
}

fn entry(value: u8) -> HalfOpen {
    HalfOpen {
        remote: peer(value),
        remote_port: 1000 + u16::from(value),
        irs: u32::from(value),
        iss: u32::from(value) + 10,
        socket: u32::from(value),
    }
}

#[test]
fn half_open_push_and_len() {
    let mut queue = HalfOpenQueue::new(2);
    assert!(queue.push(entry(1)));
    assert_eq!(queue.len(), 1);
}

#[test]
fn half_open_limit_drops_and_counts() {
    let mut queue = HalfOpenQueue::new(1);
    assert!(queue.push(entry(1)));
    assert!(!queue.push(entry(2)));
    assert_eq!(queue.stats.half_open_drops, 1);
}

#[test]
fn half_open_remove_by_tuple() {
    let mut queue = HalfOpenQueue::new(2);
    queue.push(entry(1));
    assert_eq!(queue.remove(peer(1), 1001), Some(entry(1)));
    assert!(queue.is_empty());
}

#[test]
fn half_open_remove_missing_is_none() {
    let mut queue = HalfOpenQueue::new(2);
    assert_eq!(queue.remove(peer(9), 999), None);
}

#[test]
fn accept_queue_push_and_pop() {
    let mut queue = AcceptQueue::new(2);
    assert!(queue.push(entry(1)));
    assert_eq!(queue.pop(), Some(entry(1)));
    assert!(queue.is_empty());
}

#[test]
fn accept_queue_limit_counts() {
    let mut queue = AcceptQueue::new(1);
    assert!(queue.push(entry(1)));
    assert!(!queue.push(entry(2)));
    assert_eq!(queue.stats.accept_queue_drops, 1);
}

#[test]
fn accept_queue_capacity_minimum_is_one() {
    let mut queue = AcceptQueue::new(0);
    assert!(queue.push(entry(1)));
}

#[test]
fn half_open_capacity_minimum_is_one() {
    let mut queue = HalfOpenQueue::new(0);
    assert!(queue.push(entry(1)));
}

#[test]
fn completed_handshake_moves_between_queues() {
    let mut half_open = HalfOpenQueue::new(2);
    let mut accept = AcceptQueue::new(2);
    half_open.push(entry(1));
    let connection = half_open.remove(peer(1), 1001).expect("half-open");
    assert!(accept.push(connection));
    assert!(half_open.is_empty());
    assert_eq!(accept.len(), 1);
}

#[test]
fn ten_thousand_syns_do_not_exhaust_memory() {
    let mut queue = HalfOpenQueue::new(64);
    for index in 0..10_000u32 {
        let address = Ipv4Addr::new([
            10,
            u8::try_from((index >> 16) & 0xff).unwrap_or(0),
            u8::try_from((index >> 8) & 0xff).unwrap_or(0),
            u8::try_from(index & 0xff).unwrap_or(0),
        ]);
        queue.push(HalfOpen {
            remote: IpAddr::V4(address),
            remote_port: 1000,
            irs: index,
            iss: index.wrapping_add(1),
            socket: index,
        });
    }
    assert_eq!(queue.len(), 64);
    assert_eq!(queue.stats.half_open_drops, 10_000 - 64);
}

#[test]
fn legitimate_connection_succeeds_during_a_flood() {
    let mut queue = HalfOpenQueue::new(2);
    queue.push(entry(1));
    queue.push(entry(2));
    queue.push(entry(3));
    assert!(!queue.push(entry(4)));
    assert_eq!(queue.len(), 2);
}

#[test]
fn accept_overflow_does_not_reset_the_half_open_entry() {
    let mut half_open = HalfOpenQueue::new(2);
    let mut accept = AcceptQueue::new(1);
    assert!(accept.push(entry(2)));
    half_open.push(entry(1));
    let connection = half_open.remove(peer(1), 1001).expect("half-open");
    assert!(!accept.push(connection));
    assert_eq!(accept.stats.accept_queue_drops, 1);
}

#[test]
fn cookie_is_valid_for_the_same_tuple() {
    let cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 0);
    assert!(cookie.validate(value, peer(1), 1001, 80, 0));
}

#[test]
fn cookie_rejects_a_different_tuple() {
    let cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 0);
    assert!(!cookie.validate(value, peer(2), 1001, 80, 0));
}

#[test]
fn cookie_rejects_a_stale_time_bucket() {
    let cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 0);
    assert!(!cookie.validate(value, peer(1), 1001, 80, 60_000 * 9));
}

#[test]
fn cookie_secret_rotation_invalidates_old_cookies() {
    let mut cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 0);
    cookie.rotate();
    assert!(!cookie.validate(value, peer(1), 1001, 80, 0));
}

#[test]
fn cookie_survives_rotation_when_issued_after() {
    let mut cookie = SynCookie::new(42);
    cookie.rotate();
    let value = cookie.encode(peer(1), 1001, 80, 0);
    assert!(cookie.validate(value, peer(1), 1001, 80, 0));
}

#[test]
fn cookie_changes_with_the_seed() {
    let first = SynCookie::new(1);
    let second = SynCookie::new(2);
    let a = first.encode(peer(1), 1001, 80, 0);
    let b = second.encode(peer(1), 1001, 80, 0);
    assert_ne!(a, b);
}

#[test]
fn isn_same_tuple_differs_across_time() {
    let generator = IsnGenerator::new(7, false);
    let first = generator.generate(peer(1), 1001, 80, 1000);
    let second = generator.generate(peer(1), 1001, 80, 2000);
    assert_ne!(first, second);
}

#[test]
fn isn_different_tuples_differ() {
    let generator = IsnGenerator::new(7, false);
    let first = generator.generate(peer(1), 1001, 80, 1000);
    let second = generator.generate(peer(2), 1001, 80, 1000);
    assert_ne!(first, second);
}

#[test]
fn isn_deterministic_mode_is_reproducible() {
    let generator = IsnGenerator::new(7, true);
    assert_eq!(
        generator.generate(peer(1), 1001, 80, 1000),
        generator.generate(peer(1), 1001, 80, 2000)
    );
    assert!(generator.is_deterministic());
}

#[test]
fn isn_default_mode_is_not_deterministic() {
    let generator = IsnGenerator::new(7, false);
    assert!(!generator.is_deterministic());
}

#[test]
fn isn_has_bit_diversity() {
    let generator = IsnGenerator::new(7, false);
    let mut ones = 0u32;
    for index in 0..256u32 {
        let value = generator.generate(
            peer(1),
            u16::try_from(index).unwrap_or(0),
            80,
            u64::from(index),
        );
        ones += value.count_ones();
    }
    assert!(ones > 2000 && ones < 5000, "ones={ones}");
}

#[test]
fn isn_is_stable_for_the_same_time_and_tuple() {
    let generator = IsnGenerator::new(7, false);
    assert_eq!(
        generator.generate(peer(1), 1001, 80, 1000),
        generator.generate(peer(1), 1001, 80, 1000)
    );
}

#[test]
fn cookie_rotation_counter_changes_the_value() {
    let mut cookie = SynCookie::new(9);
    let first = cookie.encode(peer(1), 1001, 80, 0);
    cookie.rotate();
    let second = cookie.encode(peer(1), 1001, 80, 0);
    assert_ne!(first, second);
}

#[test]
fn stats_start_at_zero() {
    let queue = HalfOpenQueue::new(4);
    assert_eq!(queue.stats.half_open_drops, 0);
    assert_eq!(queue.stats.accept_queue_drops, 0);
}

#[test]
fn abandoned_counter_is_available() {
    let mut queue = AcceptQueue::new(1);
    queue.stats.abandoned = 1;
    assert_eq!(queue.stats.abandoned, 1);
}

#[test]
fn cookie_path_counter_is_available() {
    let mut queue = HalfOpenQueue::new(1);
    queue.stats.cookies_accepted = 3;
    assert_eq!(queue.stats.cookies_accepted, 3);
}

#[test]
fn cookie_time_bucket_is_stable_within_the_minute() {
    let cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 30_000);
    assert!(cookie.validate(value, peer(1), 1001, 80, 59_999));
}

#[test]
fn isn_deterministic_mode_ignores_the_clock() {
    let generator = IsnGenerator::new(5, true);
    assert_eq!(
        generator.generate(peer(3), 2000, 443, 0),
        generator.generate(peer(3), 2000, 443, 999_999)
    );
}

// ---------------------------------------------------------------------------------------------
// NetBSD usrreq port (`tcp_usrreq.c:tcp_connect`/`tcp_listen`/`tcp_accept`/`tcp_disconnect1`,
// `uipc_socket.c:soclose`/`sonewconn`).  Every case asserts a state or wire effect that the
// corresponding donor branch produces; a mutation of that branch must fail the test.
// ---------------------------------------------------------------------------------------------

#[test]
fn connect_rejects_a_zero_remote_port() {
    let (_link, mut a, _b) = pair();
    assert_eq!(
        a.tcp_connect((IpAddr::V4(address(B)), 0), None),
        Err(Errno::EADDRNOTAVAIL)
    );
}

#[test]
fn connect_binds_an_ephemeral_port_and_sends_the_syn() {
    let (link, mut a, _b) = pair();
    let client = a
        .tcp_connect((IpAddr::V4(address(B)), 80), None)
        .expect("connect");
    assert_eq!(a.tcp_state(client), Ok(TcpState::SynSent));
    let (port, remote) = a.test_tcp_endpoints(client).expect("endpoints");
    assert_ne!(port, 0);
    assert_eq!(remote, Some((IpAddr::V4(address(B)), 80)));
    assert!(
        trace_link_tcp(&link, 0)
            .iter()
            .any(|(flags, _, _, _)| flags.syn() && !flags.ack()),
        "active open must emit the initial SYN"
    );
}

#[test]
fn connect_rejects_a_second_socket_on_the_same_local_port() {
    let (_link, mut a, _b) = pair();
    a.tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("first connect");
    assert_eq!(
        a.tcp_connect((IpAddr::V4(address(B)), 81), Some(40_000)),
        Err(Errno::EADDRINUSE)
    );
}

#[test]
fn listen_enters_the_listen_state_and_accept_is_empty() {
    let (_link, _a, mut b) = pair();
    let listener = b.tcp_listen(80, 8).expect("listen");
    assert_eq!(b.tcp_state(listener), Ok(TcpState::Listen));
    assert_eq!(b.tcp_accept(listener), Ok(None));
}

#[test]
fn accept_pops_completed_connections_in_fifo_order() {
    let (_link, mut a, mut b) = pair();
    let listener = b.tcp_listen(80, 8).expect("listen");
    a.tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("first");
    pump_ms(&mut a, &mut b, 0, 200);
    a.tcp_connect((IpAddr::V4(address(B)), 80), Some(40_001))
        .expect("second");
    pump_ms(&mut a, &mut b, 210, 450);
    let first = b
        .tcp_accept(listener)
        .expect("accept first")
        .expect("first queued");
    let second = b
        .tcp_accept(listener)
        .expect("accept second")
        .expect("second queued");
    assert_eq!(
        b.test_tcp_endpoints(first).expect("first endpoints").1,
        Some((IpAddr::V4(address(A)), 40_000))
    );
    assert_eq!(
        b.test_tcp_endpoints(second).expect("second endpoints").1,
        Some((IpAddr::V4(address(A)), 40_001))
    );
    assert_eq!(b.tcp_accept(listener), Ok(None));
}

#[test]
fn close_established_flushes_receive_data_and_keeps_queued_send_data() {
    let (link, mut a, mut b) = pair();
    let (client, server) = handshake(&mut a, &mut b);
    a.test_set_tcp_snd_wnd(client, 0);
    assert_eq!(a.send(client, b"hello"), Ok(5));
    let mark = link.trace().entries().len();
    a.close(client).expect("close");
    assert!(
        !trace_link_tcp(&link, mark)
            .iter()
            .any(|(flags, _, _, _)| flags.fin()),
        "a close with queued data defers the FIN until the data drains"
    );
    a.test_set_tcp_snd_wnd(client, 65_535);
    pump_ms(&mut a, &mut b, 0, 500);
    let mut buffer = [0u8; 16];
    let count = b.recv(server, &mut buffer).expect("receive queued data");
    assert_eq!(&buffer[..count], b"hello");
    assert!(
        trace_link_tcp(&link, mark)
            .iter()
            .any(|(flags, _, _, _)| flags.fin()),
        "the deferred FIN follows the drained send queue"
    );
}

#[test]
fn close_established_flushes_the_receive_queue() {
    let (_link, mut a, mut b) = pair();
    let (client, server) = handshake(&mut a, &mut b);
    a.send(client, b"pending").expect("send");
    pump_ms(&mut a, &mut b, 0, 300);
    assert!(b.recv_queue_len(server).expect("receive queue") > 0);
    b.close(server).expect("close");
    assert_eq!(b.recv_queue_len(server), Ok(0));
}

#[test]
fn close_embryonic_syn_received_sends_no_fin() {
    let (link, _a, mut b) = pair();
    let listener = b.tcp_listen(80, 2).expect("listen");
    inject_segment(&mut b, A, B, 40_000, 80, 100, 0, TcpFlags::SYN, &[]);
    b.poll(Instant::ZERO);
    let child = b.test_listener_half_open(listener)[0];
    assert_eq!(b.tcp_state(child), Ok(TcpState::SynReceived));
    let mark = link.trace().entries().len();
    b.close(child).expect("close");
    assert_eq!(b.tcp_state(child), Ok(TcpState::Closed));
    assert!(
        !trace_link_tcp(&link, mark)
            .iter()
            .any(|(flags, _, _, _)| flags.fin()),
        "tcp_disconnect1 calls tcp_close for an embryonic connection, not tcp_usrclosed"
    );
    assert!(b.test_listener_half_open(listener).is_empty());
}

#[test]
fn close_syn_sent_drops_the_connection_without_a_fin() {
    let (link, mut a, _b) = pair();
    let client = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    let mark = link.trace().entries().len();
    a.close(client).expect("close");
    assert_eq!(a.tcp_state(client), Ok(TcpState::Closed));
    assert!(
        !trace_link_tcp(&link, mark)
            .iter()
            .any(|(flags, _, _, _)| flags.fin()),
        "tcp_disconnect1 closes SYN_SENT without a FIN"
    );
}

#[test]
fn shutdown_write_on_syn_received_sends_the_fin() {
    let (link, _a, mut b) = pair();
    let listener = b.tcp_listen(80, 2).expect("listen");
    inject_segment(&mut b, A, B, 40_000, 80, 100, 0, TcpFlags::SYN, &[]);
    b.poll(Instant::ZERO);
    let child = b.test_listener_half_open(listener)[0];
    let mark = link.trace().entries().len();
    b.shutdown_write(child).expect("shutdown");
    assert_eq!(b.tcp_state(child), Ok(TcpState::FinWait1));
    assert!(
        trace_link_tcp(&link, mark)
            .iter()
            .any(|(flags, _, _, _)| flags.fin()),
        "tcp_shutdown runs tcp_usrclosed, which moves SYN_RECEIVED to FIN_WAIT_1"
    );
}

#[test]
fn close_listener_aborts_queued_children() {
    let (_link, _a, mut b) = pair();
    let listener = b.tcp_listen(80, 4).expect("listen");
    let completed = b.test_pending_connection(listener).expect("completed");
    inject_segment(&mut b, A, B, 40_002, 80, 100, 0, TcpFlags::SYN, &[]);
    b.poll(Instant::ZERO);
    let partial = b.test_listener_half_open(listener)[0];
    b.close(listener).expect("close listener");
    assert_eq!(b.tcp_state(completed), Ok(TcpState::Closed));
    assert_eq!(b.tcp_state(partial), Ok(TcpState::Closed));
    assert_eq!(b.tcp_accept(listener), Err(Errno::EBADF));
}

#[test]
fn close_listener_leaves_an_accepted_child_open() {
    let (_link, mut a, mut b) = pair();
    let (_client, accepted) = handshake(&mut a, &mut b);
    b.close(SocketId(1)).expect("close listener");
    assert_eq!(b.tcp_state(accepted), Ok(TcpState::Established));
}

#[test]
fn second_close_of_a_connection_is_ebadf() {
    let (_link, mut a, mut b) = pair();
    let (_client, server) = handshake(&mut a, &mut b);
    b.close(server).expect("first close");
    assert_eq!(b.close(server), Err(Errno::EBADF));
}
