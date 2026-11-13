// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Observability and ICMP coverage for the native backend.

mod common;

use common::{CLIENT_IP, PORT, SERVER_IP, connected_pair, setup, step};
use raven_netcore::testutil::PairConfig;
use raven_netcore::{Errno, RouteEntry, SocketState, SocketTransport};

#[test]
fn routes_are_listed_added_and_deleted() {
    let mut now = 0;
    let (mut client, _server) = setup(PairConfig::reliable(), &mut now);
    let initial = client.routes();
    assert!(
        initial.iter().any(|route| {
            route.dest.octets()[..3] == CLIENT_IP.octets()[..3] && route.prefix_len == 24
        }),
        "connected route missing: {initial:?}"
    );
    client
        .add_route(RouteEntry {
            dest: raven_netcore::Ipv4Addr::new(192, 0, 2, 0),
            prefix_len: 24,
            gateway: None,
            mtu: 1500,
        })
        .expect("add route");
    assert!(
        client
            .routes()
            .iter()
            .any(|route| route.dest.octets() == [192, 0, 2, 0] && route.prefix_len == 24)
    );
    client
        .delete_route(raven_netcore::Ipv4Addr::new(192, 0, 2, 0), 24)
        .expect("delete route");
    assert!(
        !client
            .routes()
            .iter()
            .any(|route| route.dest.octets() == [192, 0, 2, 0])
    );
    assert_eq!(
        client.delete_route(raven_netcore::Ipv4Addr::new(192, 0, 2, 0), 24),
        Err(Errno::ENOENT)
    );
}

#[test]
fn socket_table_and_interface_counters_reflect_traffic() {
    let mut now = 0;
    let (mut client, mut server, _listener, client_socket, server_socket) =
        connected_pair(PairConfig::reliable(), &mut now);
    client.send(client_socket, b"counter").unwrap();
    for _ in 0..2000 {
        let _ = step(&mut client, &mut server, &mut now);
        let table = server.socket_table();
        if let Some(entry) = table.iter().find(|entry| entry.id == server_socket)
            && entry.state == SocketState::Established
        {
            break;
        }
    }
    let table = client.socket_table();
    let entry = table
        .iter()
        .find(|entry| entry.id == client_socket)
        .expect("client socket in table");
    assert_eq!(entry.transport, SocketTransport::Tcp);
    assert_eq!(entry.state, SocketState::Established);
    assert_eq!(entry.local.map(|value| value.0), Some(CLIENT_IP));
    assert_eq!(entry.remote, Some((SERVER_IP, PORT)));

    let counters = client.interface_counters().expect("counters");
    assert!(counters.tx_packets > 0);
    assert!(counters.tx_bytes > 0);
    let _ = server.interface_counters().expect("server counters");
}

#[test]
fn icmp_echo_is_bound_sent_and_received() {
    let mut now = 0;
    let (mut client, mut server) = setup(PairConfig::reliable(), &mut now);
    let socket = client.icmp_bind(0x1234).expect("icmp bind");
    client
        .icmp_echo(socket, SERVER_IP, 0x1234, 7, b"ping")
        .expect("icmp send");
    let mut buffer = [0u8; 16];
    let mut seen = false;
    for _ in 0..2000 {
        let _ = step(&mut client, &mut server, &mut now);
        match client.icmp_recv(socket, &mut buffer) {
            Ok(length) => {
                assert_eq!(&buffer[..length], b"ping");
                seen = true;
                break;
            }
            Err(Errno::EAGAIN) => {}
            Err(error) => panic!("icmp recv: {error:?}"),
        }
    }
    assert!(seen, "no ICMP echo reply");
    let table = client.socket_table();
    assert!(table.iter().any(|entry| {
        entry.id == socket
            && entry.transport == SocketTransport::Icmp
            && entry.state == SocketState::Icmp
    }));
    assert_eq!(
        client.icmp_echo(socket, SERVER_IP, 0x9999, 8, b"bad"),
        Err(Errno::EINVAL)
    );
}
