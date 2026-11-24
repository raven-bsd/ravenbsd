// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K548 rev 39: a peer RST must not stop a listener from accepting the next connection.

mod common;

use common::{B, address, pair, pump_ms};
use raven_tcpip::{IpAddr, SocketId};

#[test]
fn a_reset_before_the_read_returns_eof_or_error_not_eagain() {
    let (_link, mut client, mut server) = pair();
    let listener = server.tcp_listen(80, 8).expect("listen");
    let first = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_031))
        .expect("connect");
    pump_ms(&mut client, &mut server, 0, 200);
    let accepted = server
        .tcp_accept(listener)
        .expect("accept")
        .expect("connection");
    assert_eq!(client.send(first, b"x"), Ok(1));
    // Host RST races the server read: the data may be discarded by the reset.
    client.abort(first).expect("reset");
    pump_ms(&mut client, &mut server, 200, 400);

    let mut buffer = [0u8; 4];
    let first_read = server.recv(accepted, &mut buffer);
    match first_read {
        Ok(0) | Err(raven_abi::errno::Errno::ECONNRESET) => {}
        Ok(count) => {
            let second_read = server.recv(accepted, &mut buffer[count..]);
            assert!(
                matches!(
                    second_read,
                    Ok(0) | Err(raven_abi::errno::Errno::ECONNRESET)
                ),
                "read after a reset must not block with EAGAIN: {second_read:?}"
            );
        }
        Err(error) => panic!("reset read returned {error:?}, expected EOF/ECONNRESET"),
    }
}

#[test]
fn listener_accepts_after_server_close_then_peer_reset() {
    let (_link, mut client, mut server) = pair();
    let listener = server.tcp_listen(80, 8).expect("listen");
    let first = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_011))
        .expect("first connect");
    pump_ms(&mut client, &mut server, 0, 200);
    let accepted = server
        .tcp_accept(listener)
        .expect("accept")
        .expect("first connection");
    assert_eq!(client.send(first, b"x"), Ok(1));
    pump_ms(&mut client, &mut server, 200, 400);
    let mut buffer = [0u8; 4];
    assert_eq!(server.recv(accepted, &mut buffer).expect("read"), 1);
    server.close(accepted).expect("server close");
    pump_ms(&mut client, &mut server, 400, 600);
    client.abort(first).expect("peer reset after close");
    pump_ms(&mut client, &mut server, 600, 800);

    let _second = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_012))
        .expect("second connect");
    pump_ms(&mut client, &mut server, 800, 1_100);
    assert!(
        server
            .tcp_accept(listener)
            .expect("listener still usable")
            .is_some(),
        "listener must accept after the peer reset the closed connection"
    );
}

#[test]
fn listener_skips_a_reset_before_accept() {
    let (_link, mut client, mut server) = pair();
    let listener = server.tcp_listen(80, 8).expect("listen");
    let first = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_021))
        .expect("first connect");
    pump_ms(&mut client, &mut server, 0, 200);
    client.abort(first).expect("reset before accept");
    pump_ms(&mut client, &mut server, 200, 400);
    let _second = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_022))
        .expect("second connect");
    pump_ms(&mut client, &mut server, 400, 700);
    assert!(
        server
            .tcp_accept(listener)
            .expect("listener still usable")
            .is_some(),
        "listener must skip the reset connection and accept the next"
    );
}

#[test]
fn listener_accepts_after_a_peer_reset() {
    let (_link, mut client, mut server) = pair();
    let listener = server.tcp_listen(80, 8).expect("listen");
    let first = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_001))
        .expect("first connect");
    pump_ms(&mut client, &mut server, 0, 200);
    let accepted = server
        .tcp_accept(listener)
        .expect("accept")
        .expect("first connection");
    assert_eq!(client.send(first, b"x"), Ok(1));
    pump_ms(&mut client, &mut server, 200, 400);
    let mut buffer = [0u8; 4];
    assert_eq!(server.recv(accepted, &mut buffer).expect("read"), 1);

    // The host peer resets the first connection after the request.
    client.abort(first).expect("abort first");
    pump_ms(&mut client, &mut server, 400, 600);

    let _second = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_002))
        .expect("second connect");
    pump_ms(&mut client, &mut server, 600, 900);
    let accepted = server
        .tcp_accept(listener)
        .expect("listener still usable")
        .expect("second connection");
    assert_ne!(accepted, SocketId(0));
}
