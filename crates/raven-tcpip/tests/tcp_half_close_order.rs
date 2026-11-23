// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K548 rev 45: bounded host-model rows for the K9 half-close/HUP contract.
//!
//! The first row does `shutdown(SHUT_WR)`, reads the peer's final bytes, then EOF; the peer half
//! close is EOF/readable but not a full HUP. The second row races a peer reset with data still in
//! flight and requires HUP after the reset.

mod common;

use common::{handshake, pair, pump_ms};
use raven_abi::errno::Errno;

#[test]
fn peer_shutdown_write_delivers_final_bytes_then_eof_in_order() {
    let (_link, mut client, mut server) = pair();
    let (client_socket, server_socket) = handshake(&mut client, &mut server);

    assert_eq!(client.send(client_socket, b"FINAL"), Ok(5));
    client
        .shutdown_write(client_socket)
        .expect("peer shutdown write");
    pump_ms(&mut client, &mut server, 0, 200);

    let mut buffer = [0u8; 16];
    let mut received = Vec::new();
    let mut eof = false;
    let mut now = 200i64;
    for _ in 0..40 {
        match server.recv(server_socket, &mut buffer) {
            Ok(0) => {
                eof = true;
                break;
            }
            Ok(count) => received.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => {
                now += 100;
                pump_ms(&mut client, &mut server, now - 100, now);
            }
            Err(error) => panic!("server read after peer half-close: {error:?}"),
        }
    }
    assert_eq!(received, b"FINAL", "final bytes must precede EOF");
    assert!(eof, "peer shutdown must produce EOF");
    let ready = server.readiness(server_socket).expect("readiness");
    assert!(ready.readable, "EOF is readable: {ready:?}");
    assert!(!ready.hup, "a peer half-close is not a full HUP: {ready:?}");

    assert_eq!(server.send(server_socket, b"REPLY"), Ok(5));
    server
        .shutdown_write(server_socket)
        .expect("server shutdown write");
    pump_ms(&mut client, &mut server, now, now + 200);
    now += 200;
    let mut reply = Vec::new();
    let mut client_eof = false;
    for _ in 0..40 {
        match client.recv(client_socket, &mut buffer) {
            Ok(0) => {
                client_eof = true;
                break;
            }
            Ok(count) => reply.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => {
                now += 100;
                pump_ms(&mut client, &mut server, now - 100, now);
            }
            Err(error) => panic!("peer read after server reply: {error:?}"),
        }
    }
    assert_eq!(reply, b"REPLY", "the peer's final bytes must precede EOF");
    assert!(client_eof, "the peer must observe EOF after its shutdown");
}

#[test]
fn peer_reset_with_data_in_flight_sets_hup_after_the_bytes() {
    let (_link, mut client, mut server) = pair();
    let (client_socket, server_socket) = handshake(&mut client, &mut server);

    assert_eq!(server.send(server_socket, b"LAST"), Ok(4));
    // Let the data leave the server queue while the peer is still connected, then reset.
    pump_ms(&mut client, &mut server, 0, 40);
    client.abort(client_socket).expect("peer reset");
    pump_ms(&mut client, &mut server, 40, 200);

    let ready = server.readiness(server_socket).expect("readiness");
    assert!(ready.hup, "a peer reset must set HUP: {ready:?}");

    let mut buffer = [0u8; 16];
    let mut received = Vec::new();
    let mut ended = false;
    let mut now = 200i64;
    for _ in 0..20 {
        match server.recv(server_socket, &mut buffer) {
            Ok(0) | Err(Errno::ECONNRESET) => {
                ended = true;
                break;
            }
            Ok(count) => received.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => {
                now += 100;
                pump_ms(&mut client, &mut server, now - 100, now);
            }
            Err(error) => panic!("server read after peer reset: {error:?}"),
        }
    }
    assert!(ended, "the reset must terminate the read");
    assert!(
        received.is_empty() || received == b"LAST",
        "bytes seen after the reset must be the final in-flight bytes, got {received:?}"
    );
}
