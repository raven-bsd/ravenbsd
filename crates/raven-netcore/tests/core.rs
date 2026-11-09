// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Core table, error and determinism tests.

mod common;

use common::{CLIENT_IP, PORT, SERVER_IP, connected_pair, setup, step};
use raven_netcore::testutil::PairConfig;
use raven_netcore::{Errno, Event, SocketId};

#[test]
fn unknown_socket_ids_return_ebadf() {
    let mut now = 0;
    let (mut client, _server) = setup(PairConfig::reliable(), &mut now);
    let unknown = SocketId(999_999);
    let mut buf = [0u8; 4];

    assert_eq!(client.recv(unknown, &mut buf), Err(Errno::EBADF));
    assert_eq!(client.send(unknown, b"x"), Err(Errno::EBADF));
    assert_eq!(client.close(unknown), Err(Errno::EBADF));
    assert_eq!(client.shutdown_write(unknown), Err(Errno::EBADF));
    assert_eq!(client.tcp_accept(unknown), Err(Errno::EBADF));
    assert_eq!(
        client.udp_send_to(unknown, b"x", (SERVER_IP, PORT)),
        Err(Errno::EBADF)
    );
    assert_eq!(client.udp_recv_from(unknown, &mut buf), Err(Errno::EBADF));
}

#[test]
fn operations_on_the_wrong_socket_type_are_rejected() {
    let mut now = 0;
    let (mut client, mut server, _listener, client_socket, _server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);
    let listener = client.tcp_listen(7002, 1).unwrap();
    let udp = client.udp_bind(7001).unwrap();
    let mut buf = [0u8; 4];

    assert_eq!(client.tcp_accept(client_socket), Err(Errno::ENOTSOCK));
    assert_eq!(client.send(listener, b"x"), Err(Errno::ENOTCONN));
    assert_eq!(client.recv(listener, &mut buf), Err(Errno::ENOTCONN));
    assert_eq!(client.shutdown_write(udp), Err(Errno::ENOTCONN));
    assert_eq!(
        client.udp_send_to(client_socket, b"x", (SERVER_IP, PORT)),
        Err(Errno::ENOTSOCK)
    );
    assert_eq!(
        server.udp_recv_from(_server_socket, &mut buf),
        Err(Errno::ENOTSOCK)
    );
}

#[test]
fn invalid_ports_are_rejected() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    assert_eq!(server.tcp_listen(0, 1), Err(Errno::EADDRNOTAVAIL));
    assert_eq!(
        client.tcp_connect((SERVER_IP, 0), None),
        Err(Errno::EADDRNOTAVAIL)
    );
    assert_eq!(
        client.tcp_connect((SERVER_IP, PORT), Some(0)),
        Err(Errno::EADDRNOTAVAIL)
    );
    assert_eq!(client.udp_bind(0), Err(Errno::EADDRNOTAVAIL));
    let udp = client.udp_bind(7001).unwrap();
    assert_eq!(
        client.udp_send_to(udp, b"x", (SERVER_IP, 0)),
        Err(Errno::EADDRNOTAVAIL)
    );
}

#[test]
fn duplicate_listener_port_is_rejected() {
    let mut now = 0;
    let (_client, mut server) = setup(PairConfig::reliable(), &mut now);
    let _listener = server.tcp_listen(PORT, 1).unwrap();
    assert_eq!(server.tcp_listen(PORT, 1), Err(Errno::EADDRINUSE));
}

#[test]
fn dhcp_can_be_enabled_and_replaced_by_static_configuration() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    client.enable_dhcp();
    client.enable_dhcp();
    for _ in 0..20 {
        now += 10;
        client.poll(now);
        server.poll(now);
    }
    assert!(client.next_poll_delay_ms(now).is_some());

    client.set_ipv4(CLIENT_IP, 24, None);
    let listener = server.tcp_listen(PORT, 2).unwrap();
    let socket = client.tcp_connect((SERVER_IP, PORT), None).unwrap();
    let accepted = common::wait_accept(&mut client, &mut server, &mut now, listener);
    assert_ne!(socket, accepted);
}

#[test]
fn identical_seeds_produce_identical_event_traces() {
    let first = trace(0x1234_5678_9abc_def0);
    let second = trace(0x1234_5678_9abc_def0);
    assert_eq!(first, second);
}

fn trace(seed: u64) -> Vec<Event> {
    let mut now = 0;
    let config = PairConfig {
        loss_per_mille: 20,
        reorder_per_mille: 10,
        delay_ms: 1,
        jitter_ms: 2,
        seed,
    };
    let (mut client, mut server) = setup(config, &mut now);
    let listener = server.tcp_listen(PORT, 2).unwrap();
    let client_socket = client.tcp_connect((SERVER_IP, PORT), None).unwrap();
    let mut events = Vec::new();

    let mut server_socket = None;
    for _ in 0..20_000 {
        let (client_events, server_events) = step(&mut client, &mut server, &mut now);
        events.extend(client_events);
        events.extend(server_events);
        if server_socket.is_none() {
            server_socket = server.tcp_accept(listener).unwrap();
        }
        if server_socket.is_some() {
            break;
        }
    }
    let server_socket = server_socket.expect("connection did not establish");

    let _ = client.send(client_socket, b"deterministic payload");
    let mut buf = [0u8; 64];
    for _ in 0..20_000 {
        let (client_events, server_events) = step(&mut client, &mut server, &mut now);
        events.extend(client_events);
        events.extend(server_events);
        if matches!(server.recv(server_socket, &mut buf), Ok(length) if length > 0) {
            break;
        }
    }

    client.close(client_socket).unwrap();
    for _ in 0..20_000 {
        let (client_events, server_events) = step(&mut client, &mut server, &mut now);
        events.extend(client_events);
        events.extend(server_events);
        if events
            .iter()
            .any(|event| matches!(event, Event::Closed(id) if *id == server_socket))
        {
            break;
        }
    }
    events
}
