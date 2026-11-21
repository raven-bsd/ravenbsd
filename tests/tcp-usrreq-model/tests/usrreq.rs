// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Listen/connect/accept/close/shutdown/RST scenarios against the public Stack API.

use raven_tcpip::{
    Errno, Instant, IpAddr, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig, TcpState,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use tcp_usrreq_model::{RefListener, completed_capacity};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

fn stack(endpoint: Endpoint, octets: [u8; 4], seed: u64) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, StackConfig::default(), seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(octets),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack
}

fn pair(seed: u64, backlog: usize) -> (Stack<Endpoint>, Stack<Endpoint>, SocketId, SocketId, Link) {
    let (client_endpoint, server_endpoint) = Link::new(LinkConfig::default(), seed);
    let link = client_endpoint.link();
    let mut client = stack(client_endpoint, A, seed.wrapping_add(1));
    let mut server = stack(server_endpoint, B, seed.wrapping_add(2));
    client.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    server.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    let listener = server.tcp_listen(80, backlog).expect("listen");
    let connected = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    pump(&mut client, &mut server, 0, 200);
    let accepted = server
        .tcp_accept(listener)
        .expect("accept")
        .expect("connection");
    (client, server, connected, accepted, link)
}

fn pump(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, from: i64, to: i64) {
    let mut time = from;
    while time <= to {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
        time += 5;
    }
}

#[test]
fn listen_rejects_bad_ports_and_closed_accept() {
    let (endpoint, _peer) = Link::new(LinkConfig::default(), 0x3210_0001);
    let mut stack = stack(endpoint, A, 1);
    assert_eq!(stack.tcp_listen(0, 1), Err(Errno::EADDRNOTAVAIL));
    let listener = stack.tcp_listen(8080, 1).expect("listen");
    assert_eq!(stack.tcp_listen(8080, 1), Err(Errno::EADDRINUSE));
    assert_eq!(stack.tcp_accept(listener), Ok(None));
    assert_eq!(stack.shutdown_write(listener), Err(Errno::ENOTCONN));
    stack.close(listener).expect("close listener");
    assert_eq!(stack.tcp_accept(listener), Err(Errno::EBADF));
    assert_eq!(stack.close(listener), Err(Errno::EBADF));
}

#[test]
fn accept_queue_is_fifo_and_bounded_by_the_backlog() {
    let (endpoint, _peer) = Link::new(LinkConfig::default(), 0x3210_0002);
    let mut stack = stack(endpoint, A, 1);
    let listener = stack.tcp_listen(8080, 1).expect("listen");
    let mut reference = RefListener::new(1);
    assert_eq!(reference.capacity(), completed_capacity(1));

    let mut expected = Vec::new();
    for _ in 0..3 {
        let id = stack.test_pending_connection(listener).expect("pending");
        if reference.push(id.0) {
            expected.push(id);
        } else {
            let _ = stack.abort(id);
            let _ = stack.close(id);
        }
    }
    assert_eq!(expected.len(), reference.capacity());
    for id in expected {
        assert_eq!(stack.tcp_accept(listener), Ok(Some(id)));
        assert_eq!(reference.accept(), Some(id.0));
    }
    assert_eq!(stack.tcp_accept(listener), Ok(None));
    assert!(reference.accept().is_none());
}

#[test]
fn backlog_overflow_with_real_handshakes_drops_the_third_child() {
    let (client_endpoint, server_endpoint) = Link::new(LinkConfig::default(), 0x3210_0003);
    let mut client = stack(client_endpoint, A, 3);
    let mut server = stack(server_endpoint, B, 4);
    client.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    server.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    let listener = server.tcp_listen(80, 1).expect("listen");
    let mut now = 0;
    for index in 0..3u16 {
        client
            .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000 + index))
            .expect("connect");
        pump(&mut client, &mut server, now, now + 300);
        now += 300;
    }
    let capacity = completed_capacity(1);
    assert_eq!(capacity, 2);
    let mut accepted = 0;
    while let Ok(Some(socket)) = server.tcp_accept(listener) {
        accepted += 1;
        let _ = server.abort(socket);
        let _ = server.close(socket);
    }
    assert_eq!(accepted, capacity, "accept queue allowed too many children");
    // The product's global `accept_queue_drops` counter only fires at the
    // `tcp.accept_queue_limit` (128), while `AcceptQueue::push` also enforces
    // the smaller backlog-derived capacity. The observable backlog bound is the
    // accepted-child count above; the counter mismatch is recorded for K271.
    assert_eq!(server.counters().accept_queue_drops, 0);
}

#[test]
fn half_open_overflow_is_counted_when_syn_cookies_are_off() {
    let (client_endpoint, server_endpoint) = Link::new(LinkConfig::default(), 0x3210_0004);
    let mut client = stack(client_endpoint, A, 5);
    let mut server = stack(server_endpoint, B, 6);
    client.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    server.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    server
        .set_property("tcp.syn_cookies_enable", 0)
        .expect("cookies off");
    let listener = server.tcp_listen(80, 1).expect("listen");
    client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_001))
        .expect("first");
    client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_002))
        .expect("second");
    pump(&mut client, &mut server, 0, 60);
    assert!(
        server.counters().half_open_drops >= 1,
        "half-open overflow was not counted: {:?}",
        server.counters()
    );
    assert_eq!(server.tcp_state(listener), Ok(TcpState::Listen));
}

#[test]
fn listener_abort_resets_a_completed_but_unaccepted_child() {
    let (client_endpoint, server_endpoint) = Link::new(LinkConfig::default(), 0x3210_0005);
    let mut client = stack(client_endpoint, A, 7);
    let mut server = stack(server_endpoint, B, 8);
    client.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    server.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    let listener = server.tcp_listen(80, 2).expect("listen");
    let connected = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    pump(&mut client, &mut server, 0, 200);
    assert_eq!(client.tcp_state(connected), Ok(TcpState::Established));
    server.close(listener).expect("listener close");
    pump(&mut client, &mut server, 200, 500);
    assert!(
        client.tcp_state(connected).is_err() || client.tcp_state(connected) == Ok(TcpState::Closed),
        "pending child survived listener close: {:?}",
        client.tcp_state(connected)
    );
    assert_eq!(server.tcp_accept(listener), Err(Errno::EBADF));
}

#[test]
fn shutdown_close_and_rst_surface_the_expected_errors() {
    let (mut client, mut server, connected, accepted, _link) = pair(0x3210_0006, 2);
    client.shutdown_write(connected).expect("shutdown");
    assert_eq!(client.tcp_state(connected), Ok(TcpState::FinWait1));
    client.close(connected).expect("close");
    assert_eq!(client.close(connected), Err(Errno::EBADF));
    pump(&mut client, &mut server, 200, 500);
    assert_eq!(server.tcp_state(accepted), Ok(TcpState::CloseWait));
    server.shutdown_write(accepted).expect("server shutdown");
    pump(&mut client, &mut server, 500, 800);
    assert!(matches!(
        client.tcp_state(connected),
        Err(_) | Ok(TcpState::Closed) | Ok(TcpState::TimeWait)
    ));

    let (mut client, mut server, connected, accepted, _link) = pair(0x3210_0007, 2);
    client.abort(connected).expect("abort");
    pump(&mut client, &mut server, 200, 500);
    assert_eq!(server.tcp_error(accepted), Ok(Some(Errno::ECONNRESET)));
}
