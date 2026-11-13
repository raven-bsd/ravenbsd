// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TCP socket behaviour over the in-memory pair.

mod common;

use common::{CLIENT_IP, PORT, SERVER_IP, connected_pair, hash, pattern, setup, step, wait_accept};
use raven_netcore::testutil::PairConfig;
use raven_netcore::{Errno, Event};

#[test]
fn tcp_echo_one_byte() {
    let mut now = 0;
    let (mut client, mut server, _listener, client_socket, server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);

    assert_eq!(client.send(client_socket, b"x").unwrap(), 1);
    let mut buf = [0u8; 8];
    let mut seen = Vec::new();
    for _ in 0..2_000 {
        let _ = step(&mut client, &mut server, &mut now);
        loop {
            match server.recv(server_socket, &mut buf) {
                Ok(0) => break,
                Ok(length) => {
                    seen.extend_from_slice(&buf[..length]);
                    assert_eq!(server.send(server_socket, &buf[..length]).unwrap(), length);
                }
                Err(Errno::EAGAIN) => break,
                Err(error) => panic!("server recv: {error:?}"),
            }
        }
        if seen.len() == 1 {
            break;
        }
    }
    assert_eq!(seen, b"x");

    let mut echoed = Vec::new();
    for _ in 0..2_000 {
        let _ = step(&mut client, &mut server, &mut now);
        loop {
            match client.recv(client_socket, &mut buf) {
                Ok(0) => break,
                Ok(length) => echoed.extend_from_slice(&buf[..length]),
                Err(Errno::EAGAIN) => break,
                Err(error) => panic!("client recv: {error:?}"),
            }
        }
        if echoed.len() == 1 {
            break;
        }
    }
    assert_eq!(echoed, b"x");
}

#[test]
fn tcp_echo_one_mib_with_loss() {
    let mut now = 0;
    let config = PairConfig {
        loss_per_mille: 20,
        reorder_per_mille: 10,
        delay_ms: 1,
        jitter_ms: 1,
        seed: 0x0bad_c0de_1234_5678,
    };
    let (mut client, mut server, _listener, client_socket, server_socket) =
        connected_pair(config, &mut now);

    let payload = pattern(1 << 20);
    let mut sent = 0;
    let mut received = Vec::new();
    let mut echoed = Vec::new();
    let mut pending_echo = Vec::new();
    let mut buf = vec![0u8; 65_536];

    for _ in 0..2_000_000 {
        if sent < payload.len() {
            match client.send(client_socket, &payload[sent..]) {
                Ok(0) => {}
                Ok(length) => sent += length,
                Err(Errno::EAGAIN) => {}
                Err(error) => panic!("client send: {error:?}"),
            }
        }

        loop {
            if pending_echo.is_empty() {
                match server.recv(server_socket, &mut buf) {
                    Ok(0) => break,
                    Ok(length) => {
                        received.extend_from_slice(&buf[..length]);
                        pending_echo.extend_from_slice(&buf[..length]);
                    }
                    Err(Errno::EAGAIN) => break,
                    Err(error) => panic!("server recv: {error:?}"),
                }
            }
            if pending_echo.is_empty() {
                break;
            }
            match server.send(server_socket, &pending_echo) {
                Ok(length) => {
                    pending_echo.drain(..length);
                }
                Err(Errno::EAGAIN) => break,
                Err(error) => panic!("server send: {error:?}"),
            }
        }

        loop {
            match client.recv(client_socket, &mut buf) {
                Ok(0) => break,
                Ok(length) => echoed.extend_from_slice(&buf[..length]),
                Err(Errno::EAGAIN) => break,
                Err(error) => panic!("client recv: {error:?}"),
            }
        }

        if sent == payload.len() && echoed.len() == payload.len() && pending_echo.is_empty() {
            break;
        }
        let _ = step(&mut client, &mut server, &mut now);
    }

    assert_eq!(sent, payload.len(), "not all bytes were sent");
    assert_eq!(
        received.len(),
        payload.len(),
        "server received a short stream"
    );
    assert_eq!(echoed.len(), payload.len(), "client received a short echo");
    assert_eq!(hash(&received), hash(&payload));
    assert_eq!(hash(&echoed), hash(&payload));
}

#[test]
fn tcp_orderly_close_from_client() {
    let mut now = 0;
    let (mut client, mut server, _listener, client_socket, server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);
    client.close(client_socket).unwrap();

    let mut buf = [0u8; 8];
    let mut eof = false;
    for _ in 0..5_000 {
        let _ = step(&mut client, &mut server, &mut now);
        match server.recv(server_socket, &mut buf) {
            Ok(0) => {
                eof = true;
                break;
            }
            Ok(_) => {}
            Err(Errno::EAGAIN) => {}
            Err(error) => panic!("server recv: {error:?}"),
        }
    }
    assert!(eof, "server did not observe EOF");
    assert_eq!(client.send(client_socket, b"x"), Err(Errno::EBADF));
}

#[test]
fn tcp_orderly_close_from_server() {
    let mut now = 0;
    let (mut client, mut server, _listener, client_socket, server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);
    server.shutdown_write(server_socket).unwrap();

    let mut buf = [0u8; 8];
    let mut eof = false;
    for _ in 0..5_000 {
        let _ = step(&mut client, &mut server, &mut now);
        match client.recv(client_socket, &mut buf) {
            Ok(0) => {
                eof = true;
                break;
            }
            Ok(_) => {}
            Err(Errno::EAGAIN) => {}
            Err(error) => panic!("client recv: {error:?}"),
        }
    }
    assert!(eof, "client did not observe EOF");
}

#[test]
fn tcp_connect_closed_port_is_refused() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let socket = client.tcp_connect((SERVER_IP, 65_000), None).unwrap();

    let mut error = None;
    for _ in 0..5_000 {
        let (events, _) = step(&mut client, &mut server, &mut now);
        for event in events {
            if let Event::Error(id, errno) = event
                && id == socket
            {
                error = Some(errno);
            }
        }
        if error.is_some() {
            break;
        }
    }
    assert_eq!(error, Some(Errno::ECONNREFUSED));
}

#[test]
fn tcp_accept_backlog_limit() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let listener = server.tcp_listen(PORT, 1).unwrap();

    let _first = client.tcp_connect((SERVER_IP, PORT), None).unwrap();
    let mut accepted = None;
    for _ in 0..5_000 {
        let (_, events) = step(&mut client, &mut server, &mut now);
        for event in events {
            if let Event::Accepted { conn, .. } = event {
                accepted = Some(conn);
            }
        }
        if accepted.is_some() {
            break;
        }
    }
    let accepted = accepted.expect("first connection did not complete");

    // The single backlog slot is occupied by the unaccepted connection.
    let second = client.tcp_connect((SERVER_IP, PORT), None).unwrap();
    let mut refused = false;
    for _ in 0..5_000 {
        let (events, _) = step(&mut client, &mut server, &mut now);
        for event in events {
            if let Event::Error(id, Errno::ECONNREFUSED) = event
                && id == second
            {
                refused = true;
            }
        }
        if refused {
            break;
        }
    }
    assert!(refused, "the second connection was not refused");

    let taken = server.tcp_accept(listener).unwrap().unwrap();
    assert_eq!(taken, accepted);

    let _third = client.tcp_connect((SERVER_IP, PORT), None).unwrap();
    let _third_server = wait_accept(&mut client, &mut server, &mut now, listener);
}

#[test]
fn tcp_send_returns_eagain_when_buffer_is_full() {
    let mut now = 0;
    let (mut client, mut server, _listener, client_socket, server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);
    let chunk = vec![0x5a; 70_000];
    let mut sent = 0;
    let full = loop {
        match client.send(client_socket, &chunk[sent..]) {
            Ok(0) => break true,
            Ok(length) => sent += length,
            Err(Errno::EAGAIN) => break true,
            Err(error) => panic!("client send: {error:?}"),
        }
    };
    assert!(full, "the send buffer never filled");
    assert!(sent <= 65_536);

    let mut buf = vec![0u8; 65_536];
    let mut drained = 0usize;
    for _ in 0..10_000 {
        let _ = step(&mut client, &mut server, &mut now);
        loop {
            match server.recv(server_socket, &mut buf) {
                Ok(0) => break,
                Ok(length) => drained += length,
                Err(Errno::EAGAIN) => break,
                Err(error) => panic!("server recv: {error:?}"),
            }
        }
        if drained > 0 {
            break;
        }
    }
    assert!(drained > 0, "server did not drain the buffer");

    let mut progressed = false;
    for _ in 0..20_000 {
        let _ = step(&mut client, &mut server, &mut now);
        loop {
            match server.recv(server_socket, &mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(Errno::EAGAIN) => break,
                Err(error) => panic!("server recv: {error:?}"),
            }
        }
        match client.send(client_socket, &chunk[sent..]) {
            Ok(0) => {}
            Ok(_) => {
                progressed = true;
                break;
            }
            Err(Errno::EAGAIN) => {}
            Err(error) => panic!("client send: {error:?}"),
        }
    }
    assert!(progressed, "send did not make progress after the peer read");
}

#[test]
fn tcp_lifecycle_event_sequence() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let listener = server.tcp_listen(PORT, 2).unwrap();
    let client_socket = client.tcp_connect((SERVER_IP, PORT), None).unwrap();

    let mut server_events = Vec::new();
    let mut connection = None;
    for _ in 0..5_000 {
        let (_, events) = step(&mut client, &mut server, &mut now);
        for event in events {
            if let Event::Accepted { conn, .. } = event {
                connection = Some(conn);
            }
            server_events.push(event);
        }
        if connection.is_some() {
            break;
        }
    }
    let connection = connection.expect("no Accepted event");
    let taken = server.tcp_accept(listener).unwrap().unwrap();
    assert_eq!(taken, connection);

    assert_eq!(client.send(client_socket, b"hi").unwrap(), 2);
    let mut buf = [0u8; 8];
    let mut read = false;
    for _ in 0..5_000 {
        let (_, events) = step(&mut client, &mut server, &mut now);
        server_events.extend(events);
        match server.recv(connection, &mut buf) {
            Ok(2) => read = true,
            Ok(_) => {}
            Err(Errno::EAGAIN) => {}
            Err(error) => panic!("server recv: {error:?}"),
        }
        if read {
            break;
        }
    }
    assert!(read);

    client.close(client_socket).unwrap();
    let mut closed = false;
    for _ in 0..5_000 {
        let (_, events) = step(&mut client, &mut server, &mut now);
        server_events.extend(events);
        if server_events
            .iter()
            .any(|event| matches!(event, Event::Closed(id) if *id == connection))
        {
            closed = true;
            break;
        }
    }
    assert!(closed, "no Closed event");

    let sequence: Vec<&Event> = server_events
        .iter()
        .filter(|event| !matches!(event, Event::Writable(_)))
        .collect();
    let accepted = sequence
        .iter()
        .position(|event| matches!(event, Event::Accepted { .. }))
        .expect("Accepted missing");
    let readable = sequence
        .iter()
        .position(|event| matches!(event, Event::Readable(id) if *id == connection))
        .expect("Readable missing");
    let closed = sequence
        .iter()
        .position(|event| matches!(event, Event::Closed(id) if *id == connection))
        .expect("Closed missing");
    assert!(accepted < readable && readable < closed);
}

#[test]
fn tcp_ephemeral_ports_are_distinct_and_explicit_conflicts_are_rejected() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let listener = server.tcp_listen(PORT, 4).unwrap();

    let first = client.tcp_connect((SERVER_IP, PORT), None).unwrap();
    let second = client.tcp_connect((SERVER_IP, PORT), None).unwrap();
    assert_ne!(first, second);
    let first_server = wait_accept(&mut client, &mut server, &mut now, listener);
    let second_server = wait_accept(&mut client, &mut server, &mut now, listener);
    assert_ne!(first_server, second_server);

    let explicit = client.tcp_connect((SERVER_IP, PORT), Some(50_000)).unwrap();
    assert_ne!(explicit, first);
    assert_eq!(
        client.tcp_connect((SERVER_IP, PORT), Some(50_000)),
        Err(Errno::EADDRINUSE)
    );
}

#[test]
fn tcp_close_invalidates_the_id() {
    let mut now = 0;
    let (mut client, _server, _listener, client_socket, _server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);
    client.close(client_socket).unwrap();
    assert_eq!(client.send(client_socket, b"x"), Err(Errno::EBADF));
    assert_eq!(client.recv(client_socket, &mut [0u8; 1]), Err(Errno::EBADF));
    assert_eq!(client.close(client_socket), Err(Errno::EBADF));
}

#[test]
fn tcp_writable_event_is_reported_after_connect() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let listener = server.tcp_listen(PORT, 2).unwrap();
    let socket = client.tcp_connect((SERVER_IP, PORT), None).unwrap();

    let mut writable = false;
    for _ in 0..5_000 {
        let (events, _) = step(&mut client, &mut server, &mut now);
        if events
            .iter()
            .any(|event| matches!(event, Event::Writable(id) if *id == socket))
        {
            writable = true;
        }
        if writable {
            break;
        }
    }
    assert!(writable, "no Writable event after connect");
    let _ = wait_accept(&mut client, &mut server, &mut now, listener);
}

#[test]
fn tcp_poll_survives_a_large_time_jump() {
    let mut now = 0;
    let (mut client, mut server, _listener, client_socket, server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);
    client.send(client_socket, b"jump").unwrap();
    now += 60_000;
    client.poll(now);
    server.poll(now);

    let mut buf = [0u8; 8];
    let mut seen = false;
    for _ in 0..5_000 {
        let _ = step(&mut client, &mut server, &mut now);
        match server.recv(server_socket, &mut buf) {
            Ok(4) => {
                assert_eq!(&buf[..4], b"jump");
                seen = true;
                break;
            }
            Ok(_) => {}
            Err(Errno::EAGAIN) => {}
            Err(error) => panic!("server recv: {error:?}"),
        }
    }
    assert!(seen, "data did not survive the time jump");
}

#[test]
fn tcp_next_poll_delay_tracks_queued_data() {
    let mut now = 0;
    let (mut client, _server, _listener, client_socket, _server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);
    client.send(client_socket, b"pending").unwrap();
    assert!(
        client.next_poll_delay_ms(now).is_some(),
        "queued data should request a poll"
    );
}

#[test]
fn tcp_set_ipv4_with_gateway_and_clamped_prefix() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    client.set_ipv4(CLIENT_IP, 40, None);
    client.set_ipv4(CLIENT_IP, 24, Some(SERVER_IP));
    let listener = server.tcp_listen(PORT, 2).unwrap();
    let socket = client.tcp_connect((SERVER_IP, PORT), None).unwrap();
    let accepted = wait_accept(&mut client, &mut server, &mut now, listener);
    assert_ne!(socket, accepted);
}

#[test]
fn tcp_endpoints_and_options_are_exposed() {
    let mut now = 0;
    let (mut client, server, listener, client_socket, server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);
    let listener_endpoint = server.local_endpoint(listener).expect("listener local");
    assert_eq!(listener_endpoint.1, PORT);
    let client_local = client.local_endpoint(client_socket).expect("client local");
    assert_eq!(client_local.0, CLIENT_IP);
    assert_eq!(client.peer_endpoint(client_socket), Some((SERVER_IP, PORT)));
    assert_eq!(
        server.peer_endpoint(server_socket),
        Some((CLIENT_IP, client_local.1))
    );

    client
        .set_socket_option(client_socket, 4, 1)
        .expect("TCP_NODELAY");
    assert_eq!(client.get_socket_option(client_socket, 4), Ok(1));
    client
        .set_socket_option(client_socket, 2, 8192)
        .expect("SO_RCVBUF");
    assert_eq!(client.get_socket_option(client_socket, 2), Ok(8192));
    let other = client.tcp_listen(PORT + 1, 1).expect("second listener");
    assert_eq!(
        client.get_socket_option(other, 2),
        Ok(0),
        "options are per socket"
    );
    assert_eq!(client.get_socket_option(client_socket, 5), Ok(0));
    assert_eq!(
        client.set_socket_option(client_socket, 99, 1),
        Err(Errno::EINVAL)
    );
}
