// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! UDP socket behaviour over the in-memory pair.

mod common;

use common::{CLIENT_IP, SERVER_IP, setup, step};
use raven_netcore::Errno;
use raven_netcore::testutil::PairConfig;

const SERVER_PORT: u16 = 7000;
const CLIENT_PORT: u16 = 7001;

#[test]
fn udp_preserves_datagram_boundaries() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let server_socket = server.udp_bind(SERVER_PORT).unwrap();
    let client_socket = client.udp_bind(CLIENT_PORT).unwrap();
    let destination = (SERVER_IP, SERVER_PORT);

    assert_eq!(
        client
            .udp_send_to(client_socket, b"first", destination)
            .unwrap(),
        5
    );
    assert_eq!(
        client
            .udp_send_to(client_socket, b"second!", destination)
            .unwrap(),
        7
    );

    let mut datagrams = Vec::new();
    let mut buf = [0u8; 64];
    for _ in 0..5_000 {
        let _ = step(&mut client, &mut server, &mut now);
        loop {
            match server.udp_recv_from(server_socket, &mut buf) {
                Ok((length, _)) => datagrams.push(buf[..length].to_vec()),
                Err(Errno::EAGAIN) => break,
                Err(error) => panic!("udp recv: {error:?}"),
            }
        }
        if datagrams.len() == 2 {
            break;
        }
    }
    assert_eq!(datagrams, vec![b"first".to_vec(), b"second!".to_vec()]);
}

#[test]
fn udp_recv_from_reports_the_source() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let server_socket = server.udp_bind(SERVER_PORT).unwrap();
    let client_socket = client.udp_bind(CLIENT_PORT).unwrap();
    client
        .udp_send_to(client_socket, b"source", (SERVER_IP, SERVER_PORT))
        .unwrap();

    let mut buf = [0u8; 32];
    let mut source = None;
    for _ in 0..5_000 {
        let _ = step(&mut client, &mut server, &mut now);
        match server.udp_recv_from(server_socket, &mut buf) {
            Ok((length, from)) => {
                assert_eq!(&buf[..length], b"source");
                source = Some(from);
                break;
            }
            Err(Errno::EAGAIN) => {}
            Err(error) => panic!("udp recv: {error:?}"),
        }
    }
    assert_eq!(source, Some((CLIENT_IP, CLIENT_PORT)));
}

#[test]
fn udp_bind_rejects_a_second_socket_on_the_same_port() {
    let mut now = 0;
    let (_client, mut server) = setup(PairConfig::reliable(), &mut now);
    let _first = server.udp_bind(SERVER_PORT).unwrap();
    assert_eq!(server.udp_bind(SERVER_PORT), Err(Errno::EADDRINUSE));
}

#[test]
fn udp_recv_on_empty_socket_returns_eagain() {
    let mut now = 0;
    let (_client, mut server) = setup(PairConfig::reliable(), &mut now);
    let socket = server.udp_bind(SERVER_PORT).unwrap();
    assert_eq!(server.recv(socket, &mut [0u8; 8]), Err(Errno::EAGAIN));
    assert_eq!(
        server.udp_recv_from(socket, &mut [0u8; 8]),
        Err(Errno::EAGAIN)
    );
}

#[test]
fn udp_send_buffer_full_returns_eagain_then_recovers() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let server_socket = server.udp_bind(SERVER_PORT).unwrap();
    let client_socket = client.udp_bind(CLIENT_PORT).unwrap();
    let destination = (SERVER_IP, SERVER_PORT);
    let payload = vec![0x42; 1_400];

    let mut queued = 0;
    let full = loop {
        match client.udp_send_to(client_socket, &payload, destination) {
            Ok(length) => {
                assert_eq!(length, payload.len());
                queued += 1;
            }
            Err(Errno::EAGAIN) => break true,
            Err(error) => panic!("udp send: {error:?}"),
        }
        assert!(queued < 100, "buffer never filled");
    };
    assert!(full && queued > 0);

    let mut buf = vec![0u8; payload.len()];
    let mut received = false;
    for _ in 0..5_000 {
        let _ = step(&mut client, &mut server, &mut now);
        match server.udp_recv_from(server_socket, &mut buf) {
            Ok((length, _)) => {
                assert_eq!(length, payload.len());
                received = true;
                break;
            }
            Err(Errno::EAGAIN) => {}
            Err(error) => panic!("udp recv: {error:?}"),
        }
    }
    assert!(received);

    let mut sent = false;
    for _ in 0..5_000 {
        let _ = step(&mut client, &mut server, &mut now);
        match client.udp_send_to(client_socket, &payload, destination) {
            Ok(_) => {
                sent = true;
                break;
            }
            Err(Errno::EAGAIN) => {}
            Err(error) => panic!("udp send: {error:?}"),
        }
    }
    assert!(sent, "send did not recover after the peer read");
}
