// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Same-scenario comparison of the smoltcp and native backends.
//!
//! The harness compares the wire-visible outcome (bytes and errors) and, per
//! socket, the ordered readiness-event sequence for the transfer, loss, orderly
//! close and refused-connection scenarios. The differences that are intentional
//! backend adaptations are pinned by their own tests.

#![cfg(all(feature = "differential", feature = "backend-raven-tcpip"))]

use std::collections::BTreeMap;

use raven_netcore::testutil::{PairConfig, PairDevice};
use raven_netcore::{Core, Errno, Event, Ipv4Addr, RavenTcpipBackend, SmoltcpBackend, SocketId};

const SERVER: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 2);
const CLIENT: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 1);
const PORT: u16 = 7000;

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    client_events: Vec<Event>,
    server_events: Vec<Event>,
    received: Vec<u8>,
    errors: Vec<Errno>,
}

fn pattern(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

fn hash(bytes: &[u8]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}

fn drive_smoltcp(config: PairConfig, payload: &[u8]) -> Outcome {
    let (device, peer) = PairDevice::pair(config);
    let mut client = Core::from_backend(SmoltcpBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    let mut server = Core::from_backend(SmoltcpBackend::new(peer, [0x02, 0, 0, 0, 0, 2], 0));
    run(&mut client, &mut server, payload)
}

fn drive_raven(config: PairConfig, payload: &[u8]) -> Outcome {
    let (device, peer) = PairDevice::pair(config);
    let mut client = Core::from_backend(RavenTcpipBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    let mut server = Core::from_backend(RavenTcpipBackend::new(peer, [0x02, 0, 0, 0, 0, 2], 0));
    run(&mut client, &mut server, payload)
}

fn run<B: raven_netcore::StackBackend>(
    client: &mut Core<B>,
    server: &mut Core<B>,
    payload: &[u8],
) -> Outcome {
    client.set_ipv4(CLIENT, 24, None);
    server.set_ipv4(SERVER, 24, None);
    let listener = server.tcp_listen(PORT, 2).expect("listen");
    let socket = client.tcp_connect((SERVER, PORT), None).expect("connect");
    let mut now = 0i64;
    let mut accepted = None;
    let mut client_events = Vec::new();
    let mut server_events = Vec::new();
    for _ in 0..10_000 {
        now += 1;
        client_events.extend(client.poll(now));
        server_events.extend(server.poll(now));
        if accepted.is_none()
            && let Ok(Some(connection)) = server.tcp_accept(listener)
        {
            accepted = Some(connection);
        }
        if accepted.is_some() {
            break;
        }
    }
    let peer = accepted.expect("accept");
    let mut sent = 0usize;
    let mut received = Vec::new();
    let mut buffer = vec![0u8; 65_536];
    for _ in 0..100_000 {
        if sent < payload.len()
            && let Ok(count) = client.send(socket, &payload[sent..])
        {
            sent += count;
        }
        loop {
            match server.recv(peer, &mut buffer) {
                Ok(0) => break,
                Ok(count) => received.extend_from_slice(&buffer[..count]),
                Err(_) => break,
            }
        }
        if received.len() >= payload.len() {
            break;
        }
        now += 1;
        client_events.extend(client.poll(now));
        server_events.extend(server.poll(now));
    }
    let mut errors = Vec::new();
    if received.len() != payload.len() || hash(&received) != hash(payload) {
        errors.push(Errno::EIO);
    }
    if client.recv(SocketId(999_999), &mut [0u8; 1]) != Err(Errno::EBADF) {
        errors.push(Errno::EINVAL);
    }
    Outcome {
        client_events,
        server_events,
        received,
        errors,
    }
}

#[test]
fn tcp_transfer_outcomes_match() {
    let payload = pattern(1024 * 1024);
    let smoltcp = drive_smoltcp(PairConfig::reliable(), &payload);
    let raven = drive_raven(PairConfig::reliable(), &payload);
    assert_eq!(raven.received, smoltcp.received);
    assert_eq!(raven.errors, smoltcp.errors);
}

#[test]
fn tcp_transfer_under_loss_outcomes_match() {
    let payload = pattern(64 * 1024);
    let config = PairConfig {
        loss_per_mille: 20,
        delay_ms: 5,
        jitter_ms: 1,
        ..PairConfig::reliable()
    };
    let smoltcp = drive_smoltcp(config, &payload);
    let raven = drive_raven(config, &payload);
    assert_eq!(raven.received, smoltcp.received);
    assert_eq!(raven.errors, smoltcp.errors);
}

#[test]
fn event_transitions_are_recorded_for_both_backends() {
    let payload = pattern(4096);
    let smoltcp = drive_smoltcp(PairConfig::reliable(), &payload);
    let raven = drive_raven(PairConfig::reliable(), &payload);
    assert!(!smoltcp.client_events.is_empty());
    assert!(!raven.client_events.is_empty());
}

fn udp_outcome<B: raven_netcore::StackBackend>(
    client: &mut Core<B>,
    server: &mut Core<B>,
) -> Vec<Vec<u8>> {
    client.set_ipv4(CLIENT, 24, None);
    server.set_ipv4(SERVER, 24, None);
    let server_socket = server.udp_bind(7001).expect("server bind");
    let client_socket = client.udp_bind(7002).expect("client bind");
    client
        .udp_send_to(client_socket, b"first", (SERVER, 7001))
        .expect("first");
    client
        .udp_send_to(client_socket, b"second!", (SERVER, 7001))
        .expect("second");
    let mut now = 0i64;
    let mut received = Vec::new();
    let mut buffer = [0u8; 64];
    for _ in 0..5_000 {
        now += 1;
        client.poll(now);
        server.poll(now);
        while let Ok((length, _)) = server.udp_recv_from(server_socket, &mut buffer) {
            received.push(buffer[..length].to_vec());
        }
        if received.len() >= 2 {
            break;
        }
    }
    received
}

#[test]
fn udp_datagram_boundaries_match() {
    let (device, peer) = PairDevice::pair(PairConfig::reliable());
    let mut client = Core::from_backend(SmoltcpBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    let mut server = Core::from_backend(SmoltcpBackend::new(peer, [0x02, 0, 0, 0, 0, 2], 0));
    let baseline = udp_outcome(&mut client, &mut server);

    let (device, peer) = PairDevice::pair(PairConfig::reliable());
    let mut client = Core::from_backend(RavenTcpipBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    let mut server = Core::from_backend(RavenTcpipBackend::new(peer, [0x02, 0, 0, 0, 0, 2], 0));
    let native = udp_outcome(&mut client, &mut server);
    assert_eq!(native, baseline);
    assert_eq!(native, vec![b"first".to_vec(), b"second!".to_vec()]);
}

/// Events grouped by the socket they describe, in order, with consecutive repeats collapsed.
///
/// The two backends emit independent events at different poll steps, so the comparison is
/// per socket: for each socket the ordered list of distinct transitions must be identical.
/// An accepted connection is attributed to its listener.
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

fn assert_same_events(name: &str, smoltcp: &Outcome, raven: &Outcome) {
    assert_eq!(
        project(&raven.client_events),
        project(&smoltcp.client_events),
        "{name}: client event sequences differ"
    );
    assert_eq!(
        project(&raven.server_events),
        project(&smoltcp.server_events),
        "{name}: server event sequences differ"
    );
}

#[test]
fn event_sequences_match_for_the_transfer_scenarios() {
    let lossy = PairConfig {
        loss_per_mille: 20,
        delay_ms: 5,
        jitter_ms: 1,
        ..PairConfig::reliable()
    };
    let scenarios = [
        ("4 KiB reliable", PairConfig::reliable(), 4096usize),
        ("1 MiB reliable", PairConfig::reliable(), 1024 * 1024),
        ("64 KiB at 2% loss", lossy, 64 * 1024),
    ];
    for (name, config, size) in scenarios {
        let payload = pattern(size);
        let smoltcp = drive_smoltcp(config, &payload);
        let raven = drive_raven(config, &payload);
        assert_same_events(name, &smoltcp, &raven);
        assert_eq!(
            project(&raven.server_events).get(&2),
            Some(&vec!["writable".to_string(), "readable".to_string()]),
            "{name}: the accepted connection becomes writable, then readable"
        );
    }
}

/// Pinned intentional difference: when the handshake completes, smoltcp reports the accepted
/// connection before its first writability, and the native backend reports writability first.
/// Both orders are legal for the socket layer, which treats the events as independent
/// transitions.
#[test]
fn accept_and_first_writable_order_is_a_pinned_difference() {
    let payload = pattern(4096);
    let smoltcp = drive_smoltcp(PairConfig::reliable(), &payload);
    let raven = drive_raven(PairConfig::reliable(), &payload);
    let listener = SocketId(1);
    let conn = SocketId(2);
    assert_eq!(
        smoltcp.server_events,
        vec![
            Event::Accepted { listener, conn },
            Event::Writable(conn),
            Event::Readable(conn)
        ]
    );
    assert_eq!(
        raven.server_events,
        vec![
            Event::Writable(conn),
            Event::Accepted { listener, conn },
            Event::Readable(conn)
        ]
    );
}

fn close_events<B: raven_netcore::StackBackend>(
    client: &mut Core<B>,
    server: &mut Core<B>,
) -> (Vec<Event>, Vec<Event>, Vec<Errno>) {
    client.set_ipv4(CLIENT, 24, None);
    server.set_ipv4(SERVER, 24, None);
    let listener = server.tcp_listen(PORT, 2).expect("listen");
    let socket = client.tcp_connect((SERVER, PORT), None).expect("connect");
    let (mut client_events, mut server_events) = (Vec::new(), Vec::new());
    let mut now = 0i64;
    let mut peer = None;
    for _ in 0..10_000 {
        now += 1;
        client_events.extend(client.poll(now));
        server_events.extend(server.poll(now));
        if peer.is_none() {
            peer = server.tcp_accept(listener).ok().flatten();
        }
        if peer.is_some() {
            break;
        }
    }
    let peer = peer.expect("accept");
    client.send(socket, b"bye").expect("send");
    let mut errors = Vec::new();
    let mut buffer = [0u8; 16];
    let mut phase = 0;
    for _ in 0..30_000 {
        now += 1;
        client_events.extend(client.poll(now));
        server_events.extend(server.poll(now));
        match phase {
            0 => {
                if server.recv(peer, &mut buffer) == Ok(3) {
                    client.shutdown_write(socket).expect("shutdown");
                    phase = 1;
                }
            }
            1 => match server.recv(peer, &mut buffer) {
                Ok(0) => {
                    server.close(peer).expect("server close");
                    client.close(socket).expect("client close");
                    phase = 2;
                }
                Ok(_) | Err(Errno::EAGAIN) => {}
                Err(error) => {
                    errors.push(error);
                    phase = 2;
                }
            },
            _ => {}
        }
    }
    (client_events, server_events, errors)
}

#[test]
fn event_sequences_match_for_an_orderly_close() {
    let (device, peer) = PairDevice::pair(PairConfig::reliable());
    let mut client = Core::from_backend(SmoltcpBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    let mut server = Core::from_backend(SmoltcpBackend::new(peer, [0x02, 0, 0, 0, 0, 2], 0));
    let baseline = close_events(&mut client, &mut server);

    let (device, peer) = PairDevice::pair(PairConfig::reliable());
    let mut client = Core::from_backend(RavenTcpipBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    let mut server = Core::from_backend(RavenTcpipBackend::new(peer, [0x02, 0, 0, 0, 0, 2], 0));
    let native = close_events(&mut client, &mut server);

    assert_eq!(native.2, baseline.2, "errors differ");
    assert_eq!(
        project(&native.0),
        project(&baseline.0),
        "client events differ"
    );
    assert_eq!(
        project(&native.1),
        project(&baseline.1),
        "server events differ"
    );
}

fn refused_events<B: raven_netcore::StackBackend>(
    client: &mut Core<B>,
    server: &mut Core<B>,
) -> (Vec<Event>, Option<Errno>) {
    client.set_ipv4(CLIENT, 24, None);
    server.set_ipv4(SERVER, 24, None);
    let socket = client.tcp_connect((SERVER, PORT), None).expect("connect");
    let mut events = Vec::new();
    let mut now = 0i64;
    for _ in 0..5_000 {
        now += 1;
        events.extend(client.poll(now));
        server.poll(now);
    }
    let outcome = client.recv(socket, &mut [0u8; 4]).err();
    (events, outcome)
}

#[test]
fn event_sequences_match_for_a_refused_connection() {
    let (device, peer) = PairDevice::pair(PairConfig::reliable());
    let mut client = Core::from_backend(SmoltcpBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    let mut server = Core::from_backend(SmoltcpBackend::new(peer, [0x02, 0, 0, 0, 0, 2], 0));
    let baseline = refused_events(&mut client, &mut server);

    let (device, peer) = PairDevice::pair(PairConfig::reliable());
    let mut client = Core::from_backend(RavenTcpipBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    let mut server = Core::from_backend(RavenTcpipBackend::new(peer, [0x02, 0, 0, 0, 0, 2], 0));
    let native = refused_events(&mut client, &mut server);

    // Pinned intentional differences: the native stack keeps the RST cause and reports
    // ECONNREFUSED where smoltcp's closed socket reports ENOTCONN, and it reports the failed
    // connect as writable and closed before the error, like a Linux poll set, where smoltcp
    // reports only the error. Both end in the same Error event.
    assert_eq!(
        (baseline.1, native.1),
        (Some(Errno::ENOTCONN), Some(Errno::ECONNREFUSED))
    );
    let error = format!("error {:?}", Errno::ECONNREFUSED);
    assert_eq!(
        project(&baseline.0).get(&1),
        Some(&vec![error.clone()]),
        "smoltcp reports only the error"
    );
    assert_eq!(
        project(&native.0).get(&1),
        Some(&vec!["writable".to_string(), "closed".to_string(), error]),
        "native reports writable, closed, error"
    );
}

/// Pinned difference: smoltcp has a DHCP client, the native stack does not, and says so.
#[test]
fn dhcp_is_reported_unsupported_by_the_native_backend() {
    let (device, _peer) = PairDevice::pair(PairConfig::reliable());
    let mut smoltcp = Core::from_backend(SmoltcpBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    assert_eq!(smoltcp.try_enable_dhcp(), Ok(()));

    let (device, _peer) = PairDevice::pair(PairConfig::reliable());
    let mut native = Core::from_backend(RavenTcpipBackend::new(device, [0x02, 0, 0, 0, 0, 1], 0));
    assert_eq!(native.try_enable_dhcp(), Err(Errno::EOPNOTSUPP));
    native.enable_dhcp();
    assert_eq!(
        native.next_poll_delay_ms(0),
        None,
        "enabling DHCP on the native backend must not arm a wake-up timer"
    );
}
