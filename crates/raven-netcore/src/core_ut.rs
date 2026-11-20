// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K225: an accepted TCP socket must transmit before the peer sends anything.

use alloc::vec::Vec;

use crate::testutil::{PairConfig, PairDevice};
use crate::{Errno, Event, Ipv4Addr, NetCore};

const SERVER_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 2);
const CLIENT_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 1);
const SERVER_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 2];
const CLIENT_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 1];
const PORT: u16 = 7000;
const BANNER: &[u8] = b"SSH-2.0-RavenBSD\r\n";

fn step(
    client: &mut NetCore<PairDevice>,
    server: &mut NetCore<PairDevice>,
    now: &mut i64,
) -> (Vec<Event>, Vec<Event>) {
    *now += 1;
    (client.poll(*now), server.poll(*now))
}

#[test]
fn accepted_socket_sends_a_banner_first() {
    let (client_device, server_device) = PairDevice::pair(PairConfig::reliable());
    let mut now = 0;
    let mut client = NetCore::new(client_device, CLIENT_MAC, now);
    let mut server = NetCore::new(server_device, SERVER_MAC, now);
    client.set_ipv4(CLIENT_IP, 24, None);
    server.set_ipv4(SERVER_IP, 24, None);

    let listener = server.tcp_listen(PORT, 4).expect("listen failed");
    let client_socket = client
        .tcp_connect((SERVER_IP, PORT), None)
        .expect("connect failed");

    let mut accepted = None;
    for _ in 0..20_000 {
        let _ = step(&mut client, &mut server, &mut now);
        if let Some(connection) = server.tcp_accept(listener).expect("accept failed") {
            accepted = Some(connection);
            break;
        }
    }
    let server_socket = accepted.expect("accept timed out");

    assert_eq!(
        server.send(server_socket, BANNER).expect("server send"),
        BANNER.len()
    );

    let mut received = Vec::new();
    let mut buffer = [0u8; 64];
    for _ in 0..2_000 {
        let _ = step(&mut client, &mut server, &mut now);
        loop {
            match client.recv(client_socket, &mut buffer) {
                Ok(0) => break,
                Ok(length) => received.extend_from_slice(&buffer[..length]),
                Err(Errno::EAGAIN) => break,
                Err(error) => panic!("client recv: {error:?}"),
            }
        }
        if received.len() == BANNER.len() {
            break;
        }
    }
    assert_eq!(received, BANNER);
}
