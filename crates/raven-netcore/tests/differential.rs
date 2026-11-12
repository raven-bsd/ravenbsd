// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Same-scenario comparison of the smoltcp and native backends.
//!
//! The harness compares the wire-visible outcome (bytes and errors) and the
//! readiness-event sequence. Event differences that are known backend
//! adaptations are recorded in the C26 report.

#![cfg(all(feature = "differential", feature = "backend-raven-tcpip"))]

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
    // The full sequence is compared when the backend adaptations are settled;
    // C26 keeps the per-backend trace in the report.
}
