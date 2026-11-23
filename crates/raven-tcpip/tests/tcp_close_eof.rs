// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K548: an accepted-socket close must deliver EOF to the peer, not EAGAIN/EIO.

mod common;

use common::{handshake, pair, pair_with, pump_ms};
use raven_abi::errno::Errno;
use raven_tcpip::StackConfig;

#[test]
fn server_close_after_one_byte_delivers_eof_without_error() {
    let (_link, mut client, mut server) = pair();
    let (client_socket, server_socket) = handshake(&mut client, &mut server);

    assert_eq!(server.send(server_socket, b"x"), Ok(1));
    pump_ms(&mut client, &mut server, 0, 200);

    let mut buffer = [0u8; 8];
    let mut received = 0usize;
    for _ in 0..20 {
        match client.recv(client_socket, &mut buffer) {
            Ok(0) => panic!("EOF before the one data byte was delivered"),
            Ok(count) => {
                received = count;
                break;
            }
            Err(Errno::EAGAIN) => pump_ms(&mut client, &mut server, 200, 400),
            Err(error) => panic!("read before close failed: {error:?}"),
        }
    }
    assert_eq!(received, 1);
    assert_eq!(&buffer[..received], b"x");

    assert_eq!(server.close(server_socket), Ok(()));
    pump_ms(&mut client, &mut server, 400, 600);

    for _ in 0..40 {
        match client.recv(client_socket, &mut buffer) {
            Ok(0) => return,
            Ok(count) => panic!("unexpected {count} bytes after the server FIN"),
            Err(Errno::EAGAIN) => pump_ms(&mut client, &mut server, 600, 800),
            Err(error) => panic!("EOF read failed after the server close: {error:?}"),
        }
    }
    panic!("client did not observe EOF after the server closed the accepted socket");
}

#[test]
fn server_close_before_client_read_delivers_byte_then_eof() {
    let (_link, mut client, mut server) = pair();
    let (client_socket, server_socket) = handshake(&mut client, &mut server);

    assert_eq!(server.send(server_socket, b"x"), Ok(1));
    assert_eq!(server.close(server_socket), Ok(()));
    pump_ms(&mut client, &mut server, 0, 400);

    let mut buffer = [0u8; 8];
    let mut received = Vec::new();
    for _ in 0..40 {
        match client.recv(client_socket, &mut buffer) {
            Ok(0) => {
                assert_eq!(received, b"x");
                return;
            }
            Ok(count) => received.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => pump_ms(&mut client, &mut server, 400, 800),
            Err(error) => panic!("client read failed after the server close: {error:?}"),
        }
    }
    panic!("client did not observe the byte then EOF after the server close");
}

#[test]
fn server_close_with_queued_output_after_peer_fin_sends_data_before_fin() {
    let config = StackConfig {
        rx_queue_bytes: 1,
        ..StackConfig::default()
    };
    let (link, mut client, mut server) = pair_with(config);
    let (client_socket, server_socket) = handshake(&mut client, &mut server);

    assert_eq!(client.send(client_socket, b"r"), Ok(1));
    client.shutdown_write(client_socket).expect("half-close");
    let mut now = 0i64;
    pump_ms(&mut client, &mut server, now, now + 200);
    now += 200;

    let mut request = [0u8; 4];
    let mut read = 0;
    for _ in 0..20 {
        match server.recv(server_socket, &mut request) {
            Ok(0) => break,
            Ok(count) => {
                read = count;
                break;
            }
            Err(Errno::EAGAIN) => {
                now += 200;
                pump_ms(&mut client, &mut server, now, now + 200);
            }
            Err(error) => panic!("server read failed: {error:?}"),
        }
    }
    assert_eq!(read, 1);
    assert_eq!(&request[..read], b"r");

    // The peer's tiny receive window lets only one byte leave the send queue; the rest is still
    // queued when close() runs in CLOSE_WAIT.
    assert_eq!(server.send(server_socket, b"0123456789"), Ok(10));
    assert_eq!(server.close(server_socket), Ok(()));
    pump_ms(&mut client, &mut server, now, now + 1_600);
    now += 1_600;

    let mut buffer = [0u8; 16];
    let mut output = Vec::new();
    for _ in 0..80 {
        match client.recv(client_socket, &mut buffer) {
            Ok(0) => {
                assert_eq!(
                    output, b"0123456789",
                    "the server FIN overtook queued output"
                );
                return;
            }
            Ok(count) => output.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => {
                now += 1_000;
                pump_ms(&mut client, &mut server, now, now + 1_000);
            }
            Err(error) => panic!("client read failed after the server close: {error:?}"),
        }
    }
    let queued_final = server.send_queue_len(server_socket);
    let client_ready = client.readiness(client_socket);
    let server_state = server.tcp_state(server_socket);
    let server_window = server.test_tcp_window(server_socket);
    let client_state = client.tcp_state(client_socket);
    let client_window = client.test_tcp_window(client_socket);
    let client_rcv_nxt = client.test_tcp_rcv_nxt(client_socket);
    panic!(
        "client did not observe EOF after the server close; output={output:?} queued_final={queued_final:?} client_ready={client_ready:?} server_state={server_state:?} server_window={server_window:?} client_state={client_state:?} client_window={client_window:?} client_rcv_nxt={client_rcv_nxt:?} trace={:?}",
        common::trace_link_tcp(&link, 0)
    );
}

#[test]
fn client_half_close_then_server_output_and_close_delivers_eof_without_error() {
    let (link, mut client, mut server) = pair();
    let (client_socket, server_socket) = handshake(&mut client, &mut server);

    assert_eq!(client.send(client_socket, b"cmd"), Ok(3));
    client.shutdown_write(client_socket).expect("half-close");
    pump_ms(&mut client, &mut server, 0, 200);

    let mut buffer = [0u8; 16];
    let mut request = Vec::new();
    for _ in 0..20 {
        match server.recv(server_socket, &mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                request.extend_from_slice(&buffer[..count]);
                if request.len() >= 3 {
                    break;
                }
            }
            Err(Errno::EAGAIN) => pump_ms(&mut client, &mut server, 200, 400),
            Err(error) => panic!("server read failed: {error:?}"),
        }
    }
    assert_eq!(request, b"cmd");

    assert_eq!(server.send(server_socket, b"output"), Ok(6));
    let queued_after_send = server.send_queue_len(server_socket).expect("send queue");
    pump_ms(&mut client, &mut server, 400, 600);
    let queued_before_close = server.send_queue_len(server_socket).expect("send queue");
    assert_eq!(server.close(server_socket), Ok(()));
    let queued_after_close = server.send_queue_len(server_socket);
    pump_ms(&mut client, &mut server, 600, 1_000);

    let mut output = Vec::new();
    for _ in 0..40 {
        match client.recv(client_socket, &mut buffer) {
            Ok(0) => {
                assert_eq!(output, b"output");
                return;
            }
            Ok(count) => output.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => pump_ms(&mut client, &mut server, 1_000, 1_400),
            Err(error) => panic!("client read failed after the server close: {error:?}"),
        }
    }
    let queued_final = server.send_queue_len(server_socket);
    let client_ready = client.readiness(client_socket);
    let server_state = server.tcp_state(server_socket);
    let server_window = server.test_tcp_window(server_socket);
    let client_state = client.tcp_state(client_socket);
    let client_window = client.test_tcp_window(client_socket);
    let client_rcv_nxt = client.test_tcp_rcv_nxt(client_socket);
    panic!(
        "client did not observe EOF after its half-close and the server close; queued_after_send={queued_after_send} queued_before_close={queued_before_close} queued_after_close={queued_after_close:?} queued_final={queued_final:?} client_ready={client_ready:?} server_state={server_state:?} server_window={server_window:?} client_state={client_state:?} client_window={client_window:?} client_rcv_nxt={client_rcv_nxt:?} trace={:?}",
        common::trace_link_tcp(&link, 0)
    );
}
