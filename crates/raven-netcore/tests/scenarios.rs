// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end scenarios for the native stack behind the socket core: byte-exact transfers
//! under adverse links, readiness-event sequences, connection lifecycle, reclaim of closed
//! sockets, backlog behaviour, UDP boundaries and determinism.

mod common;

use std::collections::BTreeMap;

use common::{CLIENT_IP, PORT, SERVER_IP, connected_pair, hash, pattern, setup, step, wait_accept};
use raven_netcore::testutil::{PairConfig, PairDevice};
use raven_netcore::{Errno, Event, NetCore, SocketId};

/// Events grouped by socket, in order, consecutive repeats collapsed. An accepted connection
/// is attributed to its listener.
fn project(events: &[Event]) -> BTreeMap<u32, Vec<String>> {
    let mut map: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for event in events {
        let (key, label) = match event {
            Event::Readable(socket) => (socket.0, "readable".to_string()),
            Event::Writable(socket) => (socket.0, "writable".to_string()),
            Event::Closed(socket) => (socket.0, "closed".to_string()),
            Event::Error(socket, errno) => (socket.0, format!("error {errno:?}")),
            Event::Accepted { listener, conn } => (listener.0, format!("accepted {}", conn.0)),
        };
        let list = map.entry(key).or_default();
        if list.last() != Some(&label) {
            list.push(label);
        }
    }
    map
}

fn labels(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

struct Transfer {
    received: Vec<u8>,
    client_events: Vec<Event>,
    server_events: Vec<Event>,
}

fn transfer(config: PairConfig, payload: &[u8]) -> Transfer {
    let mut now = 0i64;
    let (mut client, mut server) = setup(config, &mut now);
    let listener = server.tcp_listen(PORT, 2).expect("listen");
    let socket = client
        .tcp_connect((SERVER_IP, PORT), None)
        .expect("connect");
    let (mut client_events, mut server_events) = (Vec::new(), Vec::new());
    let mut peer = None;
    for _ in 0..20_000 {
        now += 1;
        client_events.extend(client.poll(now));
        server_events.extend(server.poll(now));
        peer = peer.or_else(|| server.tcp_accept(listener).ok().flatten());
        if peer.is_some() {
            break;
        }
    }
    let peer = peer.expect("accept");
    let (mut sent, mut received) = (0usize, Vec::new());
    let mut buffer = vec![0u8; 65_536];
    for _ in 0..2_000_000 {
        if sent < payload.len()
            && let Ok(count) = client.send(socket, &payload[sent..])
        {
            sent += count;
        }
        while let Ok(count) = server.recv(peer, &mut buffer) {
            if count == 0 {
                break;
            }
            received.extend_from_slice(&buffer[..count]);
        }
        if received.len() >= payload.len() {
            break;
        }
        now += 1;
        client_events.extend(client.poll(now));
        server_events.extend(server.poll(now));
    }
    Transfer {
        received,
        client_events,
        server_events,
    }
}

#[test]
fn a_reliable_megabyte_arrives_intact_with_a_stable_event_sequence() {
    let payload = pattern(1024 * 1024);
    let result = transfer(PairConfig::reliable(), &payload);
    assert_eq!(result.received.len(), payload.len());
    assert_eq!(hash(&result.received), hash(&payload));
    assert_eq!(
        project(&result.client_events),
        BTreeMap::from([(1, labels(&["writable"]))])
    );
    assert_eq!(
        project(&result.server_events),
        BTreeMap::from([
            (1, labels(&["accepted 2"])),
            (2, labels(&["writable", "readable"])),
        ])
    );
}

/// The accept notification and the first writability of the accepted connection arrive in
/// this order; the sequence is pinned so a change is a conscious one.
#[test]
fn the_server_reports_writable_before_accepted() {
    let result = transfer(PairConfig::reliable(), &pattern(4096));
    let (listener, conn) = (SocketId(1), SocketId(2));
    assert_eq!(
        result.server_events,
        vec![
            Event::Writable(conn),
            Event::Accepted { listener, conn },
            Event::Readable(conn)
        ]
    );
}

#[test]
fn a_lossy_reordering_link_still_delivers_every_byte_in_order() {
    let config = PairConfig {
        loss_per_mille: 30,
        reorder_per_mille: 100,
        delay_ms: 5,
        jitter_ms: 3,
        ..PairConfig::reliable()
    };
    let payload = pattern(256 * 1024);
    let result = transfer(config, &payload);
    assert_eq!(result.received.len(), payload.len());
    assert_eq!(hash(&result.received), hash(&payload));
}

#[test]
fn a_transfer_is_reproducible_from_the_seed() {
    let config = PairConfig {
        loss_per_mille: 20,
        delay_ms: 3,
        jitter_ms: 2,
        seed: 0xfeed_beef,
        ..PairConfig::reliable()
    };
    let payload = pattern(64 * 1024);
    let first = transfer(config, &payload);
    let second = transfer(config, &payload);
    assert_eq!(first.client_events, second.client_events);
    assert_eq!(first.server_events, second.server_events);
    assert_eq!(first.received, second.received);
}

#[test]
fn bytes_flow_in_both_directions_at_once() {
    let mut now = 0i64;
    let (mut client, mut server, _l, a, b) = connected_pair(PairConfig::delayed(2, 1, 7), &mut now);
    let up = pattern(100_000);
    let down: Vec<u8> = pattern(90_000).iter().map(|byte| !byte).collect();
    let (mut up_sent, mut down_sent) = (0usize, 0usize);
    let (mut at_server, mut at_client) = (Vec::new(), Vec::new());
    let mut buffer = vec![0u8; 8192];
    for _ in 0..200_000 {
        if up_sent < up.len()
            && let Ok(count) = client.send(a, &up[up_sent..])
        {
            up_sent += count;
        }
        if down_sent < down.len()
            && let Ok(count) = server.send(b, &down[down_sent..])
        {
            down_sent += count;
        }
        while let Ok(count) = server.recv(b, &mut buffer) {
            if count == 0 {
                break;
            }
            at_server.extend_from_slice(&buffer[..count]);
        }
        while let Ok(count) = client.recv(a, &mut buffer) {
            if count == 0 {
                break;
            }
            at_client.extend_from_slice(&buffer[..count]);
        }
        if at_server.len() == up.len() && at_client.len() == down.len() {
            break;
        }
        step(&mut client, &mut server, &mut now);
    }
    assert_eq!(at_server, up);
    assert_eq!(at_client, down);
}

fn drive_close(
    client: &mut NetCore<PairDevice>,
    server: &mut NetCore<PairDevice>,
    now: &mut i64,
    client_socket: SocketId,
    server_socket: SocketId,
) -> (Vec<Event>, Vec<Event>) {
    let (mut client_events, mut server_events) = (Vec::new(), Vec::new());
    client.send(client_socket, b"bye").expect("send");
    let mut buffer = [0u8; 16];
    let mut phase = 0;
    for _ in 0..30_000 {
        let (c, s) = step(client, server, now);
        client_events.extend(c);
        server_events.extend(s);
        match phase {
            0 if server.recv(server_socket, &mut buffer) == Ok(3) => {
                client.shutdown_write(client_socket).expect("shutdown");
                phase = 1;
            }
            1 if server.recv(server_socket, &mut buffer) == Ok(0) => {
                server.close(server_socket).expect("server close");
                client.close(client_socket).expect("client close");
                phase = 2;
            }
            _ => {}
        }
    }
    assert_eq!(phase, 2, "the orderly close did not complete");
    (client_events, server_events)
}

#[test]
fn an_orderly_close_ends_both_sides_and_invalidates_the_ids() {
    let mut now = 0i64;
    let (mut client, mut server, _l, a, b) = connected_pair(PairConfig::reliable(), &mut now);
    let (client_events, server_events) = drive_close(&mut client, &mut server, &mut now, a, b);
    assert!(
        project(&server_events)
            .get(&b.0)
            .is_some_and(|list| list.contains(&"readable".to_string()))
    );
    assert!(
        !project(&client_events)
            .values()
            .flatten()
            .any(|event| event.starts_with("error")),
        "an orderly close must not raise an error: {client_events:?}"
    );
    let mut buffer = [0u8; 4];
    assert_eq!(client.recv(a, &mut buffer), Err(Errno::EBADF));
    assert_eq!(server.recv(b, &mut buffer), Err(Errno::EBADF));
}

#[test]
fn a_refused_connection_reports_econnrefused_with_writable_closed_error() {
    let mut now = 0i64;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let socket = client
        .tcp_connect((SERVER_IP, PORT), None)
        .expect("connect");
    let mut events = Vec::new();
    for _ in 0..5_000 {
        now += 1;
        events.extend(client.poll(now));
        server.poll(now);
    }
    assert_eq!(
        project(&events).get(&socket.0),
        Some(&labels(&[
            "writable",
            "closed",
            &format!("error {:?}", Errno::ECONNREFUSED)
        ]))
    );
    assert_eq!(client.recv(socket, &mut [0u8; 4]), Err(Errno::ECONNREFUSED));
}

#[test]
fn many_short_connections_do_not_exhaust_the_socket_table() {
    let mut now = 0i64;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let listener = server.tcp_listen(PORT, 4).expect("listen");
    let mut buffer = [0u8; 8];
    for round in 0..150u32 {
        let socket = client
            .tcp_connect((SERVER_IP, PORT), None)
            .unwrap_or_else(|error| panic!("connect {round}: {error:?}"));
        let peer = wait_accept(&mut client, &mut server, &mut now, listener);
        client.send(socket, b"ping").expect("send");
        let mut got = 0;
        for _ in 0..2_000 {
            step(&mut client, &mut server, &mut now);
            if let Ok(count) = server.recv(peer, &mut buffer) {
                got = count;
                break;
            }
        }
        assert_eq!(got, 4, "round {round}");
        client.close(socket).expect("client close");
        server.close(peer).expect("server close");
        for _ in 0..40 {
            step(&mut client, &mut server, &mut now);
        }
    }
}

#[test]
fn simultaneous_connections_keep_their_streams_apart() {
    let mut now = 0i64;
    let (mut client, mut server) = setup(PairConfig::delayed(1, 1, 3), &mut now);
    let listener = server.tcp_listen(PORT, 8).expect("listen");
    let mut pairs = Vec::new();
    for index in 0..6u8 {
        let socket = client
            .tcp_connect((SERVER_IP, PORT), None)
            .expect("connect");
        let peer = wait_accept(&mut client, &mut server, &mut now, listener);
        pairs.push((index, socket, peer));
    }
    for (index, socket, _) in &pairs {
        let data = vec![*index; 3_000 + usize::from(*index) * 100];
        let mut sent = 0;
        while sent < data.len() {
            if let Ok(count) = client.send(*socket, &data[sent..]) {
                sent += count;
            }
            step(&mut client, &mut server, &mut now);
        }
    }
    for _ in 0..500 {
        step(&mut client, &mut server, &mut now);
    }
    for (index, _, peer) in &pairs {
        let mut got = Vec::new();
        let mut buffer = [0u8; 4096];
        while let Ok(count) = server.recv(*peer, &mut buffer) {
            if count == 0 {
                break;
            }
            got.extend_from_slice(&buffer[..count]);
        }
        assert_eq!(got.len(), 3_000 + usize::from(*index) * 100);
        assert!(got.iter().all(|byte| byte == index), "stream {index} mixed");
    }
}

#[test]
fn a_full_backlog_does_not_break_the_connections_already_accepted() {
    let mut now = 0i64;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let listener = server.tcp_listen(PORT, 1).expect("listen");
    let first = client.tcp_connect((SERVER_IP, PORT), None).expect("first");
    let accepted = wait_accept(&mut client, &mut server, &mut now, listener);
    for _ in 0..4 {
        let _ = client.tcp_connect((SERVER_IP, PORT), None);
    }
    for _ in 0..300 {
        step(&mut client, &mut server, &mut now);
    }
    client.send(first, b"still alive").expect("send");
    let mut buffer = [0u8; 32];
    let mut got = 0;
    for _ in 0..500 {
        step(&mut client, &mut server, &mut now);
        if let Ok(count) = server.recv(accepted, &mut buffer) {
            got = count;
            break;
        }
    }
    assert_eq!(&buffer[..got], b"still alive");
}

#[test]
fn udp_datagram_boundaries_and_sources_are_preserved() {
    let mut now = 0i64;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let receiver = server.udp_bind(7001).expect("bind");
    let sender = client.udp_bind(7002).expect("bind");
    for datagram in [&b"first"[..], b"second!", b"", b"x"] {
        client
            .udp_send_to(sender, datagram, (SERVER_IP, 7001))
            .expect("send");
    }
    let mut received = Vec::new();
    let mut buffer = [0u8; 64];
    for _ in 0..200 {
        step(&mut client, &mut server, &mut now);
        while let Ok((length, from)) = server.udp_recv_from(receiver, &mut buffer) {
            assert_eq!(from, (CLIENT_IP, 7002));
            received.push(buffer[..length].to_vec());
        }
    }
    assert_eq!(
        received,
        vec![
            b"first".to_vec(),
            b"second!".to_vec(),
            Vec::new(),
            b"x".to_vec()
        ]
    );
}

#[test]
fn dhcp_is_reported_unsupported_and_arms_no_timer() {
    let mut now = 0i64;
    let (mut client, _server) = setup(PairConfig::reliable(), &mut now);
    assert_eq!(client.try_enable_dhcp(), Err(Errno::EOPNOTSUPP));
    client.enable_dhcp();
    assert_eq!(client.next_poll_delay_ms(now), None);
}

#[test]
fn an_idle_stack_asks_for_no_wakeups() {
    let mut now = 0i64;
    let (mut client, mut server, _l, _a, _b) = connected_pair(PairConfig::reliable(), &mut now);
    for _ in 0..100 {
        step(&mut client, &mut server, &mut now);
    }
    let delay = client.next_poll_delay_ms(now);
    assert!(
        delay.is_none_or(|ms| ms > 0),
        "an idle connection must not demand an immediate poll: {delay:?}"
    );
}
