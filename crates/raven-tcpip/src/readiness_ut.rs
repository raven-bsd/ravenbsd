// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Live readiness tests: non-consuming, direction-specific, backlog, FIN/error/UDP edges.

use super::*;
use crate::Ipv4Config;
use crate::stack::{Ipv4Addr, StackConfig};
use crate::testutil::LoopbackDevice;
use crate::time::Instant;

fn stack() -> Stack<LoopbackDevice> {
    let mut stack = Stack::new(
        LoopbackDevice::new(),
        StackConfig::default(),
        7,
        Instant::ZERO,
    );
    stack
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new([127, 0, 0, 1]),
            prefix_len: 8,
            gateway: None,
        })
        .expect("set_ipv4");
    stack
}

fn established() -> (Stack<LoopbackDevice>, SocketId) {
    let mut stack = stack();
    let listener = stack.tcp_listen(7000, 4).expect("listen");
    let connection = stack
        .test_pending_connection(listener)
        .expect("pending connection");
    (stack, connection)
}

#[test]
fn a_query_does_not_consume_received_bytes() {
    let (mut stack, connection) = established();
    stack.test_push_rx(connection, b"hello").expect("push");

    let first = stack.readiness(connection).expect("first");
    let second = stack.readiness(connection).expect("second");
    assert_eq!(first, second);
    assert!(first.readable, "buffered data is readable");
    assert!(!first.hup, "data is not a hangup");
    assert!(!first.error);

    let mut buffer = [0u8; 8];
    assert_eq!(stack.recv(connection, &mut buffer), Ok(5));
    assert_eq!(&buffer[..5], b"hello");
    let after = stack.readiness(connection).expect("after");
    assert!(!after.readable, "the drained established socket is idle");
}

#[test]
fn readable_and_writable_do_not_alias() {
    let (mut stack, connection) = established();
    let idle = stack.readiness(connection).expect("idle");
    assert!(!idle.readable);
    assert!(idle.writable, "an idle established socket has send space");

    stack.test_push_rx(connection, b"x").expect("push");
    let readable = stack.readiness(connection).expect("readable");
    assert!(readable.readable);
    assert!(readable.writable);

    stack.shutdown_write(connection).expect("shutdown");
    let shut = stack.readiness(connection).expect("shut");
    assert!(!shut.writable, "the write side is shut down");
}

#[test]
fn a_listener_is_readable_only_while_the_backlog_has_a_connection() {
    let mut stack = stack();
    let listener = stack.tcp_listen(7000, 4).expect("listen");
    assert!(!stack.readiness(listener).expect("empty").readable);
    assert!(!stack.readiness(listener).expect("empty").writable);

    let connection = stack
        .test_pending_connection(listener)
        .expect("pending connection");
    assert!(stack.readiness(listener).expect("backlog").readable);
    assert!(!stack.readiness(listener).expect("backlog").writable);

    assert_eq!(stack.tcp_accept(listener), Ok(Some(connection)));
    assert!(!stack.readiness(listener).expect("accepted").readable);
}

#[test]
fn a_peer_fin_is_readable_without_claiming_a_full_hangup() {
    let (mut stack, connection) = established();
    stack.test_set_peer_fin(connection, true).expect("fin");

    let eof = stack.readiness(connection).expect("eof");
    assert!(eof.readable, "EOF is readable");
    assert!(!eof.hup, "a peer half-close is not a full hangup");

    // Unread data plus FIN: data first, EOF after the drain.
    stack.test_push_rx(connection, b"x").expect("push");
    let unread = stack.readiness(connection).expect("unread");
    assert!(unread.readable);
    assert!(!unread.hup);
    let mut buffer = [0u8; 2];
    assert_eq!(stack.recv(connection, &mut buffer), Ok(1));
    assert!(stack.readiness(connection).expect("after").readable);
    assert!(!stack.readiness(connection).expect("after").hup);

    stack.test_set_closed(connection, true).expect("full close");
    let hup = stack.readiness(connection).expect("hup");
    assert!(hup.hup, "a full peer close is a hangup");
}

#[test]
fn a_pending_error_is_reported_and_not_consumed_by_the_query() {
    let (mut stack, connection) = established();
    stack
        .test_set_error(connection, Some(Errno::ECONNRESET))
        .expect("error");

    assert!(stack.readiness(connection).expect("first").error);
    assert!(stack.readiness(connection).expect("second").error);
    let mut buffer = [0u8; 2];
    assert_eq!(stack.recv(connection, &mut buffer), Err(Errno::ECONNRESET));
    assert!(!stack.readiness(connection).expect("after").error);
}

#[test]
fn udp_readiness_is_the_datagram_queue() {
    let mut stack = stack();
    let socket = stack.udp_bind(7001).expect("udp bind");
    let idle = stack.readiness(socket).expect("idle");
    assert!(!idle.readable);
    assert!(idle.writable);

    stack
        .test_push_udp(socket, b"datagram", (Ipv4Addr::new([127, 0, 0, 1]), 7002))
        .expect("push udp");
    assert!(stack.readiness(socket).expect("first").readable);
    assert!(stack.readiness(socket).expect("second").readable);

    let mut buffer = [0u8; 16];
    let (length, _source) = stack.udp_recv_from(socket, &mut buffer).expect("recv");
    assert_eq!(length, 8);
    assert!(!stack.readiness(socket).expect("drained").readable);
}

#[test]
fn an_unknown_socket_is_ebadf() {
    let stack = stack();
    assert_eq!(stack.readiness(SocketId(999)), Err(Errno::EBADF));
}
