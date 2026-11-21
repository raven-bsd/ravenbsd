// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Live readiness tests: non-consuming, direction-specific, backlog, FIN/error/UDP edges.

use alloc::vec::Vec;

use super::*;
use crate::Ipv4Config;
use crate::event::{POLLIN, POLLOUT, POLLRDBAND, POLLRDNORM, POLLWRNORM};
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
    assert!(
        shut.writable,
        "FreeBSD sowriteable keeps POLLOUT set on SBS_CANTSENDMORE; a write returns EPIPE"
    );
}

#[test]
fn an_error_is_readable_and_writable_like_freebsd_soreadable_sowriteable() {
    let (mut stack, connection) = established();
    stack
        .test_set_error(connection, Some(Errno::ECONNRESET))
        .expect("error");

    let ready = stack.readiness(connection).expect("ready");
    assert!(ready.readable, "FreeBSD soreadable includes so_error");
    assert!(ready.writable, "FreeBSD sowriteable includes so_error");
    assert!(ready.error);
    assert!(!ready.hup);
}

#[test]
fn a_connecting_socket_is_not_writable_until_connected() {
    let mut stack = stack();
    let connection = stack
        .tcp_connect((Ipv4Addr::new([127, 0, 0, 2]), 7002), Some(7001))
        .expect("connect");
    let ready = stack.readiness(connection).expect("connecting");
    assert!(
        !ready.writable,
        "FreeBSD sowriteable requires SS_ISCONNECTED for a connection-required socket; \
         state={:?} error={:?} ready={ready:?}",
        stack.tcp_state(connection),
        stack.tcp_error(connection)
    );
    assert!(!ready.readable);
}

#[test]
fn the_live_query_does_not_read_the_previous_edge_baseline() {
    let (mut stack, connection) = established();
    {
        let record = stack.sockets.record_mut(connection).expect("record");
        record.prev_readable = true;
        record.prev_writable = false;
    }
    let ready = stack.readiness(connection).expect("live");
    assert!(
        !ready.readable,
        "the stored edge baseline must not decide the live readiness"
    );
    assert!(ready.writable);
}

#[test]
fn an_error_raises_readable_and_writable_edges_from_the_same_predicate() {
    let (mut stack, connection) = established();
    stack
        .test_set_error(connection, Some(Errno::ECONNRESET))
        .expect("error");
    stack.poll(Instant::ZERO);
    let events: Vec<crate::Event> = stack.take_events().collect();
    assert!(
        events.contains(&crate::Event::Readable(connection)),
        "{events:?}"
    );
    assert!(
        events.contains(&crate::Event::Writable(connection)),
        "{events:?}"
    );
    assert!(
        events.contains(&crate::Event::Error(connection, Errno::ECONNRESET)),
        "{events:?}"
    );
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
fn sodopoll_maps_the_netbsd_event_bits() {
    let (mut stack, connection) = established();
    assert_eq!(
        stack
            .sodopoll(connection, POLLIN | POLLOUT | POLLRDNORM | POLLWRNORM)
            .expect("poll"),
        POLLOUT
    );
    stack.test_push_rx(connection, b"x").expect("push");
    assert_eq!(
        stack
            .sodopoll(connection, POLLIN | POLLOUT | POLLRDNORM | POLLWRNORM)
            .expect("poll"),
        POLLIN | POLLRDNORM | POLLOUT
    );
    assert_eq!(
        stack.sodopoll(connection, POLLRDBAND).expect("rdband"),
        0,
        "Raven has no priority-band queue"
    );
}

#[test]
fn sopoll_registers_only_the_unready_directions() {
    let (stack, connection) = established();
    let (mask, read, write) = stack.sopoll(connection, POLLOUT).expect("poll out");
    assert_eq!(mask, POLLOUT);
    assert!(!read && !write, "a ready socket is not registered");

    let (mask, read, write) = stack.sopoll(connection, POLLIN).expect("poll in");
    assert_eq!(mask, 0);
    assert!(read && !write, "the idle read direction is registered");

    let (mask, read, write) = stack
        .sopoll(connection, POLLIN | POLLOUT)
        .expect("poll both");
    assert_eq!(mask, POLLOUT);
    assert!(!read && !write);
}

#[test]
fn sopoll_uses_the_live_predicate_not_the_stored_edge_baseline() {
    let (mut stack, connection) = established();
    {
        let record = stack.sockets.record_mut(connection).expect("record");
        record.prev_readable = true;
        record.prev_writable = false;
    }
    let (mask, read, _) = stack.sopoll(connection, POLLIN).expect("poll");
    assert_eq!(mask, 0, "stored edge baseline must not mask the live query");
    assert!(read);
}

#[test]
fn netbsd_sowritable_requires_a_connection_or_a_shutdown() {
    let (mut stack, connection) = established();
    assert!(stack.readiness(connection).expect("idle").writable);
    stack.shutdown_write(connection).expect("shutdown");
    let shut = stack.readiness(connection).expect("shut");
    assert!(
        shut.writable,
        "NetBSD sowritable returns true for SS_CANTSENDMORE"
    );
    assert_eq!(stack.sodopoll(connection, POLLOUT).expect("poll"), POLLOUT);
}

#[test]
fn a_pending_error_is_reported_and_not_consumed_by_the_query() {
    let (mut stack, connection) = established();
    stack
        .test_set_error(connection, Some(Errno::ECONNRESET))
        .expect("error");

    assert!(stack.readiness(connection).expect("first").error);
    assert!(stack.readiness(connection).expect("first").readable);
    assert!(stack.readiness(connection).expect("first").writable);
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
fn listener_readiness_bits_are_not_transposed() {
    let mut stack = stack();
    let listener = stack.tcp_listen(7100, 4).expect("listen");
    let empty = stack.readiness(listener).expect("empty");
    assert!(!empty.readable);
    assert!(!empty.writable);

    let _connection = stack
        .test_pending_connection(listener)
        .expect("pending connection");
    let backlog = stack.readiness(listener).expect("backlog");
    assert!(backlog.readable, "a pending connection is readable");
    assert!(!backlog.writable, "a listener is never writable");
}

#[test]
fn a_listener_backlog_raises_an_accepted_edge() {
    let mut stack = stack();
    let listener = stack.tcp_listen(7101, 4).expect("listen");
    stack.poll(Instant::ZERO);
    let _ = stack.take_events().count();

    let connection = stack
        .test_pending_connection(listener)
        .expect("pending connection");
    stack.poll(Instant::from_millis(1));
    let events: Vec<crate::Event> = stack.take_events().collect();
    assert!(
        events.contains(&crate::Event::Accepted {
            listener,
            conn: connection
        }),
        "{events:?}"
    );

    stack.poll(Instant::from_millis(2));
    let second: Vec<crate::Event> = stack.take_events().collect();
    assert!(
        !second
            .iter()
            .any(|event| matches!(event, crate::Event::Accepted { .. })),
        "stale accepted event: {second:?}"
    );
}

#[test]
fn a_new_data_byte_raises_a_readable_edge() {
    let (mut stack, connection) = established();
    stack.poll(Instant::ZERO);
    let _ = stack.take_events().count();

    stack.test_push_rx(connection, b"x").expect("push");
    stack.poll(Instant::from_millis(1));
    let events: Vec<crate::Event> = stack.take_events().collect();
    assert!(
        events.contains(&crate::Event::Readable(connection)),
        "missed readable edge: {events:?}"
    );
}

#[test]
fn a_close_event_is_emitted_once_and_not_stale() {
    let (mut stack, connection) = established();
    stack.poll(Instant::ZERO);
    let _ = stack.take_events().count();

    stack.test_set_closed(connection, true).expect("close");
    stack.poll(Instant::from_millis(1));
    let first: Vec<crate::Event> = stack.take_events().collect();
    assert!(
        first.contains(&crate::Event::Closed(connection)),
        "missed close edge: {first:?}"
    );

    stack.poll(Instant::from_millis(2));
    let second: Vec<crate::Event> = stack.take_events().collect();
    assert!(
        !second.contains(&crate::Event::Closed(connection)),
        "stale close event: {second:?}"
    );
}

#[test]
fn an_unknown_socket_is_ebadf() {
    let stack = stack();
    assert_eq!(stack.readiness(SocketId(999)), Err(Errno::EBADF));
}
