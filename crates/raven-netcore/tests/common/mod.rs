// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared test harness for the in-memory pair.

#![allow(dead_code)]

use raven_netcore::testutil::{PairConfig, PairDevice};
use raven_netcore::{Event, Ipv4Addr, NetCore, SocketId};

pub const SERVER_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 2);
pub const CLIENT_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 1);
pub const SERVER_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 2];
pub const CLIENT_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 1];
pub const PORT: u16 = 7000;

pub fn setup(config: PairConfig, now: &mut i64) -> (NetCore<PairDevice>, NetCore<PairDevice>) {
    let (client_device, server_device) = PairDevice::pair(config);
    let mut client = NetCore::new(client_device, CLIENT_MAC, *now);
    let mut server = NetCore::new(server_device, SERVER_MAC, *now);
    client.set_ipv4(CLIENT_IP, 24, None);
    server.set_ipv4(SERVER_IP, 24, None);
    (client, server)
}

pub fn step(
    client: &mut NetCore<PairDevice>,
    server: &mut NetCore<PairDevice>,
    now: &mut i64,
) -> (Vec<Event>, Vec<Event>) {
    *now += 1;
    (client.poll(*now), server.poll(*now))
}

pub fn wait_accept(
    client: &mut NetCore<PairDevice>,
    server: &mut NetCore<PairDevice>,
    now: &mut i64,
    listener: SocketId,
) -> SocketId {
    for _ in 0..20_000 {
        let _ = step(client, server, now);
        if let Some(conn) = server.tcp_accept(listener).expect("accept failed") {
            return conn;
        }
    }
    panic!("TCP accept timed out");
}

pub fn connected_pair(
    config: PairConfig,
    now: &mut i64,
) -> (
    NetCore<PairDevice>,
    NetCore<PairDevice>,
    SocketId,
    SocketId,
    SocketId,
) {
    let (mut client, mut server) = setup(config, now);
    let listener = server.tcp_listen(PORT, 4).expect("listen failed");
    let client_socket = client
        .tcp_connect((SERVER_IP, PORT), None)
        .expect("connect failed");
    let server_socket = wait_accept(&mut client, &mut server, now, listener);
    (client, server, listener, client_socket, server_socket)
}

pub fn hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn pattern(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}
