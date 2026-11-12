// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C32: a closed connection in TIME-WAIT must stop counting against the socket table, yet
//! keep answering the peer until the 2*MSL timer ends.

#![cfg(feature = "testutil")]

mod common;

use common::{A, B, handshake, inject_segment, pair_with, pair_with_link, pump_ms, trace_link_tcp};
use raven_tcpip::{Instant, SocketId, Stack, StackConfig};
use raven_tcpip_sim::{Endpoint, LinkConfig};
use raven_tcpip_wire::tcp::TcpFlags;

fn small_table() -> StackConfig {
    StackConfig {
        max_sockets: 6,
        ..StackConfig::default()
    }
}

/// Close from the client side, let the FIN exchange finish, and return the client's socket
/// (now in TIME-WAIT) with its final sequence state.
fn active_close(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    client: SocketId,
    server: SocketId,
    now: i64,
) -> i64 {
    a.close(client).expect("client close");
    let mut time = now;
    pump_ms(a, b, time, time + 100);
    time += 100;
    let mut buffer = [0u8; 8];
    assert_eq!(b.recv(server, &mut buffer), Ok(0), "the server sees EOF");
    b.close(server).expect("server close");
    pump_ms(a, b, time, time + 200);
    time + 200
}

#[test]
fn a_time_wait_connection_is_detached_when_the_table_fills() {
    let (_link, mut a, mut b) = pair_with(small_table());
    let (client, server) = handshake(&mut a, &mut b);
    let now = active_close(&mut a, &mut b, client, server, 200);
    assert_eq!(a.test_time_wait_entries(), (1, 0), "held with its record");
    for round in 0..8 {
        let socket = a
            .udp_bind(0)
            .unwrap_or_else(|error| panic!("bind {round}: {error:?}"));
        a.close(socket).expect("close");
    }
    a.poll(Instant::from_millis(now + 1));
    assert_eq!(
        a.test_time_wait_entries(),
        (1, 1),
        "now only the entry remains"
    );
}

#[test]
fn a_detached_time_wait_still_acks_a_retransmitted_fin() {
    let (link, mut a, mut b) = pair_with(small_table());
    let (client, server) = handshake(&mut a, &mut b);
    let now = active_close(&mut a, &mut b, client, server, 200);
    let (_wnd, _una, snd_nxt) = a.test_tcp_window(client).expect("window");
    let rcv_nxt = a.test_tcp_rcv_nxt(client).expect("rcv_nxt");
    for _ in 0..8 {
        let socket = a.udp_bind(0).expect("bind");
        a.close(socket).expect("close");
    }
    a.poll(Instant::from_millis(now + 1));
    assert_eq!(a.test_time_wait_entries().1, 1);
    let start = link.trace().entries().len();
    inject_segment(
        &mut a,
        B,
        A,
        80,
        40_000,
        rcv_nxt.wrapping_sub(1),
        snd_nxt,
        TcpFlags::from_bits(TcpFlags::FIN.bits() | TcpFlags::ACK.bits()),
        &[],
    );
    let acks: Vec<_> = trace_link_tcp(&link, start)
        .into_iter()
        .filter(|(flags, _, _, _)| flags.ack() && !flags.syn() && !flags.fin())
        .collect();
    assert!(
        acks.iter()
            .any(|(_, seq, ack, _)| *seq == snd_nxt && *ack == rcv_nxt),
        "no ACK of the retransmitted FIN: {acks:?}"
    );
}

#[test]
fn a_detached_time_wait_ignores_a_reset_and_drops_stale_data() {
    let (link, mut a, mut b) = pair_with(small_table());
    let (client, server) = handshake(&mut a, &mut b);
    let now = active_close(&mut a, &mut b, client, server, 200);
    let rcv_nxt = a.test_tcp_rcv_nxt(client).expect("rcv_nxt");
    let (_wnd, _una, snd_nxt) = a.test_tcp_window(client).expect("window");
    for _ in 0..8 {
        let socket = a.udp_bind(0).expect("bind");
        a.close(socket).expect("close");
    }
    a.poll(Instant::from_millis(now + 1));
    let start = link.trace().entries().len();
    inject_segment(
        &mut a,
        B,
        A,
        80,
        40_000,
        rcv_nxt,
        snd_nxt,
        TcpFlags::RST,
        &[],
    );
    assert_eq!(
        a.test_time_wait_entries(),
        (1, 1),
        "the RST did not kill it"
    );
    assert!(
        trace_link_tcp(&link, start).is_empty(),
        "an RST in TIME-WAIT is silently ignored"
    );
    assert!(a.counters().timewait_rst_ignored >= 1);
}

#[test]
fn a_detached_time_wait_expires_and_the_port_is_reusable() {
    let (_link, mut a, mut b) = pair_with(small_table());
    let (client, server) = handshake(&mut a, &mut b);
    let now = active_close(&mut a, &mut b, client, server, 200);
    for _ in 0..8 {
        let socket = a.udp_bind(0).expect("bind");
        a.close(socket).expect("close");
    }
    a.poll(Instant::from_millis(now + 1));
    assert_eq!(a.test_time_wait_entries(), (1, 1));
    let long_after = now + 70_000;
    a.poll(Instant::from_millis(long_after));
    assert_eq!(a.test_time_wait_entries(), (0, 0), "2*MSL elapsed");
}

#[test]
fn a_hundred_short_connections_never_exhaust_a_small_table() {
    let (_link, mut a, mut b) = pair_with_link(
        LinkConfig {
            rtt_ms: 2,
            ..LinkConfig::default()
        },
        small_table(),
    );
    b.tcp_listen(80, 4).expect("listen");
    let mut time = 0i64;
    let mut buffer = [0u8; 8];
    for round in 0..120u16 {
        let client = a
            .tcp_connect((raven_tcpip::IpAddr::V4(common::address(B)), 80), None)
            .unwrap_or_else(|error| panic!("connect {round}: {error:?}"));
        pump_ms(&mut a, &mut b, time, time + 40);
        time += 40;
        let server = b
            .tcp_accept(SocketId(1))
            .expect("accept")
            .unwrap_or_else(|| {
                panic!(
                    "round {round}: no pending connection; client {:?} server drops {:?} listen_drops {} entries {:?}",
                    a.counters().out_errors,
                    b.counters().half_open_drops,
                    b.counters().listen_drops,
                    a.test_time_wait_entries()
                )
            });
        a.close(client).expect("client close");
        pump_ms(&mut a, &mut b, time, time + 40);
        time += 40;
        assert_eq!(b.recv(server, &mut buffer), Ok(0));
        b.close(server).expect("server close");
        pump_ms(&mut a, &mut b, time, time + 40);
        time += 40;
    }
    assert!(
        a.test_time_wait_entries().0 > 0,
        "TIME-WAIT is being tracked"
    );
    assert!(a.udp_bind(0).is_ok(), "the table still has room");
}
