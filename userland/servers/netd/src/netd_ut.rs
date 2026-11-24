// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the netd file table, loopback TCP/UDP and interface control.

use super::*;
use crate::Loopback;
use raven_proto_net::socket::{READY_ERROR, READY_HUP, READY_READABLE, READY_WRITABLE};
use raven_proto_net::{IfName, IfaceRequest, IfaceResponse, Route, Shutdown, SockOpt};

const MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 1];
const SERVER: Endpoint = Endpoint {
    ip: [127, 0, 0, 1],
    port: 7000,
};
const UDP_SERVER: Endpoint = Endpoint {
    ip: [127, 0, 0, 1],
    port: 7001,
};
const UDP_CLIENT: Endpoint = Endpoint {
    ip: [127, 0, 0, 1],
    port: 7002,
};

const NIC_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 2];

fn new_netd() -> Netd<Loopback> {
    Netd::new(Loopback::new(MAC), MAC, 0)
}

fn new_netd_with_nic() -> Netd<Loopback> {
    let mut netd = new_netd();
    netd.attach_nic(Loopback::new(NIC_MAC), NicConfig::qemu(NIC_MAC, 1500), 0);
    netd
}

fn call_request(netd: &mut Netd<Loopback>, file: u64, request: &SocketRequest<'_>) -> Vec<u8> {
    let mut encoded = [0u8; 512];
    let length = request.encode(&mut encoded).expect("request encode");
    let mut response = [0u8; 2048];
    let read = netd
        .call(file, &encoded[..length], &mut response)
        .unwrap_or_else(|error| panic!("call {request:?}: {error:?}"));
    response[..read].to_vec()
}

fn call_socket(netd: &mut Netd<Loopback>, file: u64, request: Call) -> Response {
    let response = call_request(netd, file, &SocketRequest::Call(request));
    Response::decode(&response).expect("response decode")
}

fn call_iface(netd: &mut Netd<Loopback>, file: u64, request: IfaceRequest) -> Vec<u8> {
    let mut encoded = [0u8; 256];
    let length = request.encode(&mut encoded).expect("iface encode");
    let mut response = [0u8; 1024];
    let read = netd
        .call(file, &encoded[..length], &mut response)
        .expect("iface call");
    response[..read].to_vec()
}

fn poll(netd: &mut Netd<Loopback>, now: &mut i64) {
    *now = now.saturating_add(1);
    let _ = netd.poll(*now);
}

fn accept(netd: &mut Netd<Loopback>, listener: u64, now: &mut i64) -> u32 {
    for _ in 0..200 {
        poll(netd, now);
        match call_socket(netd, listener, Call::Accept) {
            Response::Socket { id } => return id,
            Response::Err(Errno::EAGAIN) => continue,
            other => panic!("unexpected accept response: {other:?}"),
        }
    }
    panic!("accept timed out");
}

fn read_retry(netd: &mut Netd<Loopback>, file: u64, buffer: &mut [u8], now: &mut i64) -> usize {
    for _ in 0..200 {
        match netd.read(file, 0, buffer) {
            Ok(read) => return read,
            Err(Errno::EAGAIN) => poll(netd, now),
            Err(error) => panic!("read failed: {error:?}"),
        }
    }
    panic!("read timed out");
}

#[test]
fn control_lists_loopback_and_address() {
    let mut netd = new_netd();
    let control = netd.open("net:").unwrap();
    let response = call_iface(&mut netd, control, IfaceRequest::List);
    let count = IfaceResponse::decode(&response[..8]).expect("count");
    assert_eq!(count, IfaceResponse::Count { count: 1 });
    let name_len = response[14] as usize;
    let iface_len = 21 + name_len;
    let iface = IfaceResponse::decode(&response[8..8 + iface_len]).expect("iface");
    match iface {
        IfaceResponse::Iface(info) => {
            assert_eq!(info.name.as_str(), "lo");
            assert_eq!(info.addr, [127, 0, 0, 1]);
            assert_eq!(info.prefix, 8);
        }
        other => panic!("unexpected iface response: {other:?}"),
    }
    netd.close(control).unwrap();
}

#[test]
fn tcp_loopback_echo_and_orderly_close() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let id = accept(&mut netd, listener, &mut now);
    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();
    let client_local = call_socket(&mut netd, client, Call::GetSockName);
    assert_eq!(
        call_socket(&mut netd, accepted, Call::GetPeerName),
        client_local
    );

    assert_eq!(netd.write(client, 0, b"hello").unwrap(), 5);
    let mut buffer = [0u8; 16];
    let read = read_retry(&mut netd, accepted, &mut buffer, &mut now);
    assert_eq!(&buffer[..read], b"hello");
    assert_eq!(netd.write(accepted, 0, b"hello").unwrap(), 5);
    let read = read_retry(&mut netd, client, &mut buffer, &mut now);
    assert_eq!(&buffer[..read], b"hello");

    assert_eq!(
        call_socket(
            &mut netd,
            client,
            Call::Shutdown {
                how: Shutdown::Write
            }
        ),
        Response::Ok
    );
    let read = read_retry(&mut netd, accepted, &mut buffer, &mut now);
    assert_eq!(read, 0);

    netd.close(accepted).unwrap();
    netd.close(client).unwrap();
    netd.close(listener).unwrap();
    assert_eq!(netd.socket_files(), 0);
}

fn read_to_eof(netd: &mut Netd<Loopback>, file: u64, now: &mut i64) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 2048];
    for _ in 0..400 {
        match netd.read(file, 0, &mut buffer) {
            Ok(0) => return output,
            Ok(count) => output.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => poll(netd, now),
            Err(error) => panic!("read before EOF failed: {error:?}"),
        }
    }
    panic!("EOF timed out after {} bytes", output.len());
}

fn server_output_connection() -> (Netd<Loopback>, u64, u64, u64, i64) {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let id = accept(&mut netd, listener, &mut now);
    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();

    // The SSH client sends its request and half-closes before the server writes the
    // command output. That puts the accepted socket in CLOSE_WAIT when it closes.
    assert_eq!(netd.write(client, 0, b"r").unwrap(), 1);
    assert_eq!(
        call_socket(
            &mut netd,
            client,
            Call::Shutdown {
                how: Shutdown::Write
            }
        ),
        Response::Ok
    );
    let mut request = [0u8; 4];
    let read = read_retry(&mut netd, accepted, &mut request, &mut now);
    assert_eq!(&request[..read], b"r");
    assert_eq!(read_retry(&mut netd, accepted, &mut request, &mut now), 0);
    (netd, listener, client, accepted, now)
}

#[test]
fn tcp_server_write_then_shutdown_delivers_all_output_before_eof() {
    let (mut netd, listener, client, accepted, mut now) = server_output_connection();
    let payload: Vec<u8> = (0..300u16).map(|value| value as u8).collect();
    assert_eq!(netd.write(accepted, 0, &payload).unwrap(), payload.len());
    assert_eq!(
        call_socket(
            &mut netd,
            accepted,
            Call::Shutdown {
                how: Shutdown::Write
            }
        ),
        Response::Ok
    );

    let received = read_to_eof(&mut netd, client, &mut now);
    assert_eq!(received, payload, "server output must precede the EOF");

    netd.close(accepted).unwrap();
    netd.close(client).unwrap();
    netd.close(listener).unwrap();
}

#[test]
fn tcp_server_close_after_output_delivers_all_output_before_eof() {
    let (mut netd, listener, client, accepted, mut now) = server_output_connection();
    let payload: Vec<u8> = (0..300u16).map(|value| (value % 251) as u8).collect();
    assert_eq!(netd.write(accepted, 0, &payload).unwrap(), payload.len());
    netd.close(accepted).unwrap();

    let received = read_to_eof(&mut netd, client, &mut now);
    assert_eq!(
        received, payload,
        "server output must precede the close/EOF"
    );

    netd.close(client).unwrap();
    netd.close(listener).unwrap();
}

#[test]
fn nic_core_serves_non_loopback_paths_and_lists_eth0() {
    let mut netd = new_netd_with_nic();
    let endpoint = Endpoint {
        ip: [10, 0, 2, 15],
        port: 7000,
    };
    let listener = netd.open("net:tcp/10.0.2.15:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 2 }),
        Response::Ok
    );
    let client = netd.open("net:tcp/10.0.2.15:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(endpoint)),
        Response::Ok
    );
    let mut now = 0;
    let id = accept(&mut netd, listener, &mut now);
    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();

    assert_eq!(netd.write(client, 0, b"nic").unwrap(), 3);
    let mut buffer = [0u8; 8];
    let read = read_retry(&mut netd, accepted, &mut buffer, &mut now);
    assert_eq!(&buffer[..read], b"nic");
    assert_eq!(netd.write(accepted, 0, &buffer[..read]).unwrap(), 3);
    let read = read_retry(&mut netd, client, &mut buffer, &mut now);
    assert_eq!(&buffer[..read], b"nic");
    match call_socket(&mut netd, client, Call::GetSockName) {
        Response::Endpoint(endpoint) => assert_eq!(endpoint.ip, [10, 0, 2, 15]),
        other => panic!("unexpected getsockname: {other:?}"),
    }

    let control = netd.open("net:").unwrap();
    let response = call_iface(&mut netd, control, IfaceRequest::List);
    let count = IfaceResponse::decode(response.get(..8).unwrap()).unwrap();
    assert_eq!(count, IfaceResponse::Count { count: 2 });
    let name_len = response[14] as usize;
    let iface_len = 21 + name_len;
    let lo = IfaceResponse::decode(response.get(8..8 + iface_len).unwrap()).unwrap();
    let IfaceResponse::Iface(lo) = lo else {
        panic!("missing lo");
    };
    assert_eq!(lo.name.as_str(), "lo");
    let offset = 8 + iface_len;
    let name_len = response[offset + 6] as usize;
    let eth = IfaceResponse::decode(response.get(offset..offset + 21 + name_len).unwrap()).unwrap();
    let IfaceResponse::Iface(eth) = eth else {
        panic!("missing eth0");
    };
    assert_eq!(eth.name.as_str(), "eth0");
    assert_eq!(eth.addr, [10, 0, 2, 15]);
    assert_eq!(eth.prefix, 24);
    assert_eq!(eth.mac, NIC_MAC);
    assert_eq!(eth.mtu, 1500);

    netd.close(accepted).unwrap();
    netd.close(client).unwrap();
    netd.close(listener).unwrap();
}

#[test]
fn lease_timers_are_relative_and_static_leases_use_the_sentinel() {
    assert_eq!(lease_timer_ms(Some(5_000), 1_000), 4_000);
    assert_eq!(lease_timer_ms(Some(500), 1_000), 0);
    assert_eq!(lease_timer_ms(None, 1_000), DHCP_TIMER_UNKNOWN);
}

#[test]
fn icmp_echo_reports_payload_source_and_rtt() {
    let mut netd = new_netd();
    let file = netd.open("net:icmp/127.0.0.1").unwrap();
    let response = call_request(
        &mut netd,
        file,
        &SocketRequest::IcmpEcho {
            id: 0x1234,
            seq: 7,
            timeout_ms: 2_000,
            data: b"ping",
        },
    );
    assert_eq!(Response::decode(&response), Ok(Response::Ok));

    let mut now = 0;
    let mut reply = None;
    for _ in 0..200 {
        poll(&mut netd, &mut now);
        let response = call_request(&mut netd, file, &SocketRequest::Read { len: 16 });
        if response.get(4..6) == Some(&[6, 0]) {
            continue;
        }
        let header = Response::decode(response.get(..22).unwrap()).unwrap();
        if let Response::Icmp {
            source,
            id,
            seq,
            rtt_ms,
            len,
        } = header
        {
            reply = Some((source, id, seq, rtt_ms, len));
            assert_eq!(&response[22..22 + len as usize], b"ping");
            break;
        }
    }
    let (source, id, seq, rtt_ms, len) = reply.expect("no ICMP reply");
    assert_eq!(
        source,
        Endpoint {
            ip: [127, 0, 0, 1],
            port: 0
        }
    );
    assert_eq!(id, 0x1234);
    assert_eq!(seq, 7);
    assert_eq!(len, 4);
    assert!(rtt_ms < 2_000);

    let control = netd.open("net:").unwrap();
    let response = call_iface(&mut netd, control, IfaceRequest::SocketList);
    let count = decode_count(&response);
    assert!(count >= 1);
    let mut offset = 8;
    let mut seen = false;
    for _ in 0..count {
        let name_len = response[offset + 12] as usize;
        let remote = response[offset + 19 + name_len] == 1;
        let length = if remote { 34 + name_len } else { 28 + name_len };
        let record = IfaceResponse::decode(&response[offset..offset + length]).unwrap();
        if let IfaceResponse::Socket(entry) = record
            && entry.transport == raven_proto_net::SOCKET_ICMP
            && entry.state == raven_proto_net::SOCKET_STATE_ICMP
        {
            seen = true;
        }
        offset += length;
    }
    assert!(seen, "ICMP socket missing from table");

    let response = call_iface(
        &mut netd,
        control,
        IfaceRequest::Counters {
            name: IfName::new("lo").unwrap(),
        },
    );
    let counters = IfaceResponse::decode(&response).unwrap();
    let IfaceResponse::Counters(_) = counters else {
        panic!("missing counters");
    };
    netd.close(file).unwrap();
    netd.close(control).unwrap();
}

#[test]
fn routes_list_add_and_delete() {
    let mut netd = new_netd();
    let control = netd.open("net:").unwrap();
    let response = call_iface(&mut netd, control, IfaceRequest::RouteList);
    assert_eq!(decode_count(&response), 1);
    let first = decode_route(&response, 8);
    assert_eq!(first.dest, [127, 0, 0, 0]);
    assert_eq!(first.prefix, 8);
    assert_eq!(first.iface.as_str(), "lo");
    assert_eq!(
        first.flags & raven_proto_net::ROUTE_UP,
        raven_proto_net::ROUTE_UP
    );

    let route = Route {
        dest: [192, 0, 2, 0],
        prefix: 24,
        gateway: None,
    };
    let response = call_iface(
        &mut netd,
        control,
        IfaceRequest::RouteAddOn {
            name: IfName::new("lo").unwrap(),
            route,
        },
    );
    assert_eq!(IfaceResponse::decode(&response), Ok(IfaceResponse::Ok));
    let response = call_iface(&mut netd, control, IfaceRequest::RouteList);
    assert_eq!(decode_count(&response), 2);
    let mut found = false;
    let mut offset = 8;
    for _ in 0..2 {
        let record = decode_route(&response, offset);
        if record.dest == [192, 0, 2, 0] && record.prefix == 24 {
            found = true;
        }
        offset += route_info_len(&response, offset);
    }
    assert!(found);

    let response = call_iface(
        &mut netd,
        control,
        IfaceRequest::RouteDelOn {
            name: IfName::new("lo").unwrap(),
            dest: [192, 0, 2, 0],
            prefix: 24,
        },
    );
    assert_eq!(IfaceResponse::decode(&response), Ok(IfaceResponse::Ok));
    let response = call_iface(&mut netd, control, IfaceRequest::RouteList);
    assert_eq!(decode_count(&response), 1);
    netd.close(control).unwrap();
}

fn decode_count(response: &[u8]) -> u16 {
    match IfaceResponse::decode(response.get(..8).unwrap()).unwrap() {
        IfaceResponse::Count { count } => count,
        other => panic!("unexpected count: {other:?}"),
    }
}

fn route_info_len(response: &[u8], offset: usize) -> usize {
    let name_len = response[offset + 12] as usize;
    let flags = response[offset + 11];
    13 + name_len
        + if flags & raven_proto_net::ROUTE_GATEWAY != 0 {
            4
        } else {
            0
        }
}

fn decode_route(response: &[u8], offset: usize) -> RouteInfo {
    let length = route_info_len(response, offset);
    match IfaceResponse::decode(&response[offset..offset + length]).unwrap() {
        IfaceResponse::RouteInfo(route) => route,
        other => panic!("unexpected route: {other:?}"),
    }
}

#[test]
fn udp_loopback_datagram_and_source_endpoint() {
    let mut netd = new_netd();
    let server = netd.open("net:udp/127.0.0.1:7001").unwrap();
    let client = netd.open("net:udp/127.0.0.1:7002").unwrap();
    assert_eq!(
        call_socket(&mut netd, server, Call::Bind(UDP_SERVER)),
        Response::Ok
    );
    assert_eq!(
        call_socket(&mut netd, client, Call::Bind(UDP_CLIENT)),
        Response::Ok
    );
    let response = call_request(
        &mut netd,
        client,
        &SocketRequest::Write {
            data: b"ping",
            to: Some(UDP_SERVER),
        },
    );
    assert_eq!(Response::decode(&response), Ok(Response::Write { len: 4 }));

    let response = call_request(&mut netd, server, &SocketRequest::Read { len: 32 });
    let header = Response::decode(&response[..17]).expect("read header");
    match header {
        Response::Read { len, from } => {
            assert_eq!(&response[17..17 + len as usize], b"ping");
            assert_eq!(
                from,
                Some(Endpoint {
                    ip: [127, 0, 0, 1],
                    port: 7002
                })
            );
        }
        other => panic!("unexpected udp read: {other:?}"),
    }
    netd.close(client).unwrap();
    netd.close(server).unwrap();
}

#[test]
fn socket_options_and_endpoints_are_reported() {
    let mut netd = new_netd();
    let file = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(
            &mut netd,
            file,
            Call::SetSockOpt {
                opt: SockOpt::RcvBuf,
                value: 4096
            }
        ),
        Response::Ok
    );
    assert_eq!(
        call_socket(
            &mut netd,
            file,
            Call::GetSockOpt {
                opt: SockOpt::RcvBuf
            }
        ),
        Response::Value(4096)
    );
    assert_eq!(
        call_socket(&mut netd, file, Call::GetSockName),
        Response::Endpoint(SERVER)
    );
    assert_eq!(
        call_socket(&mut netd, file, Call::GetPeerName),
        Response::Err(Errno::ENOTCONN)
    );
    netd.close(file).unwrap();
}

#[test]
fn malformed_requests_and_unknown_files_do_not_panic() {
    let mut netd = new_netd();
    let file = netd.open("net:").unwrap();
    let mut response = [0u8; 64];
    assert_eq!(
        netd.call(file, &[0xff, 0xff], &mut response),
        Err(Errno::EINVAL)
    );
    assert_eq!(netd.read(999, 0, &mut response), Err(Errno::EBADF));
    assert_eq!(netd.write(999, 0, &[0]), Err(Errno::EBADF));
    assert_eq!(netd.close(999), Err(Errno::EBADF));
    assert_eq!(netd.open("net:accept/01"), Err(Errno::EINVAL));
    assert_eq!(netd.open("net:accept/999"), Err(Errno::ENOENT));
}

#[test]
fn a_fresh_server_can_rebind_after_drop() {
    let mut first = new_netd();
    let file = first.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut first, file, Call::Listen { backlog: 1 }),
        Response::Ok
    );
    drop(first);
    let mut second = new_netd();
    let file = second.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut second, file, Call::Listen { backlog: 1 }),
        Response::Ok
    );
}

/// S174: a server that closed first leaves 127.0.0.1:7000 in TIME_WAIT. The port must not be
/// reusable by a plain socket, and must be reusable with `SO_REUSEADDR`, in that order.
#[test]
fn a_time_wait_port_rebinds_only_with_so_reuseaddr() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let id = accept(&mut netd, listener, &mut now);
    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();

    // The server closes first, so its local port 7000 enters TIME_WAIT.
    netd.close(accepted).unwrap();
    netd.close(client).unwrap();
    for _ in 0..20 {
        poll(&mut netd, &mut now);
    }
    netd.close(listener).unwrap();

    let plain = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, plain, Call::Bind(SERVER)),
        Response::Ok
    );
    assert_eq!(
        call_socket(&mut netd, plain, Call::Listen { backlog: 4 }),
        Response::Err(Errno::EADDRINUSE)
    );
    netd.close(plain).unwrap();

    let reuse = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(
            &mut netd,
            reuse,
            Call::SetSockOpt {
                opt: SockOpt::ReuseAddr,
                value: 1
            }
        ),
        Response::Ok
    );
    assert_eq!(
        call_socket(&mut netd, reuse, Call::Bind(SERVER)),
        Response::Ok
    );
    assert_eq!(
        call_socket(&mut netd, reuse, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    netd.close(reuse).unwrap();
}

/// S174: `SO_ERROR` reports the pending `ECONNREFUSED` once, then 0.
#[test]
fn so_error_reports_connection_refused_once_then_zero() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let refused = i32::try_from(-Errno::ECONNREFUSED.to_ret()).unwrap_or(0);
    let mut first = Response::Value(0);
    for _ in 0..100 {
        poll(&mut netd, &mut now);
        first = call_socket(
            &mut netd,
            client,
            Call::GetSockOpt {
                opt: SockOpt::Error,
            },
        );
        if first != Response::Value(0) {
            break;
        }
    }
    assert_eq!(first, Response::Value(refused));
    assert_eq!(
        call_socket(
            &mut netd,
            client,
            Call::GetSockOpt {
                opt: SockOpt::Error,
            },
        ),
        Response::Value(0)
    );
    netd.close(client).unwrap();
}

fn test_lease(address: Ipv4Addr, prefix: u8, router: Option<Ipv4Addr>, dns: Ipv4Addr) -> DhcpLease {
    DhcpLease {
        address,
        prefix_len: prefix,
        router,
        dns: alloc::vec![dns],
        renew_at_ms: Some(1_000),
        rebind_at_ms: Some(2_000),
        expires_at_ms: Some(3_000),
    }
}

#[test]
fn a_platform_nic_is_not_published_before_a_lease() {
    let mut netd = new_netd();
    netd.attach_nic(
        Loopback::new(NIC_MAC),
        NicConfig::platform(NIC_MAC, 1500),
        0,
    );
    assert!(netd.published_nic().is_none());
    let control = netd.open("net:").unwrap();
    let response = call_iface(&mut netd, control, IfaceRequest::List);
    assert_eq!(
        IfaceResponse::decode(&response[..8]).unwrap(),
        IfaceResponse::Count { count: 1 }
    );
    let name = IfName::new("eth0").unwrap();
    let response = call_iface(&mut netd, control, IfaceRequest::Get { name });
    assert_eq!(
        IfaceResponse::decode(&response).unwrap(),
        IfaceResponse::Err(Errno::ENODEV)
    );
    let response = call_iface(&mut netd, control, IfaceRequest::DhcpLease { name });
    assert_eq!(
        IfaceResponse::decode(&response).unwrap(),
        IfaceResponse::Err(Errno::ENOENT)
    );
}

#[test]
fn a_valid_platform_lease_is_applied_and_published() {
    let mut netd = new_netd();
    netd.attach_nic(
        Loopback::new(NIC_MAC),
        NicConfig::platform(NIC_MAC, 1500),
        0,
    );
    let lease = test_lease(
        Ipv4Addr::new(192, 168, 1, 23),
        24,
        Some(Ipv4Addr::new(192, 168, 1, 1)),
        Ipv4Addr::new(192, 168, 1, 1),
    );
    assert!(netd.apply_nic_lease(&lease));
    let config = netd.published_nic().unwrap();
    assert_eq!(config.addr, [192, 168, 1, 23]);
    assert_eq!(config.prefix, 24);
    assert_eq!(config.gateway, Some([192, 168, 1, 1]));
    assert_eq!(
        netd.dhcp_lease().unwrap().address.octets(),
        [192, 168, 1, 23]
    );

    let control = netd.open("net:").unwrap();
    let response = call_iface(&mut netd, control, IfaceRequest::List);
    assert_eq!(
        IfaceResponse::decode(&response[..8]).unwrap(),
        IfaceResponse::Count { count: 2 }
    );
}

#[test]
fn an_invalid_platform_lease_is_rejected() {
    let mut netd = new_netd();
    netd.attach_nic(
        Loopback::new(NIC_MAC),
        NicConfig::platform(NIC_MAC, 1500),
        0,
    );
    let cases = [
        test_lease(
            Ipv4Addr::UNSPECIFIED,
            24,
            Some(Ipv4Addr::new(10, 0, 2, 2)),
            Ipv4Addr::new(10, 0, 2, 3),
        ),
        test_lease(
            Ipv4Addr::new(224, 0, 0, 1),
            24,
            Some(Ipv4Addr::new(10, 0, 2, 2)),
            Ipv4Addr::new(10, 0, 2, 3),
        ),
        test_lease(
            Ipv4Addr::new(192, 168, 1, 23),
            0,
            Some(Ipv4Addr::new(192, 168, 1, 1)),
            Ipv4Addr::new(192, 168, 1, 1),
        ),
        test_lease(
            Ipv4Addr::new(192, 168, 1, 23),
            24,
            Some(Ipv4Addr::new(0, 0, 0, 0)),
            Ipv4Addr::new(192, 168, 1, 1),
        ),
        test_lease(
            Ipv4Addr::new(192, 168, 1, 23),
            24,
            Some(Ipv4Addr::new(192, 168, 1, 1)),
            Ipv4Addr::UNSPECIFIED,
        ),
    ];
    for lease in cases {
        assert!(!netd.apply_nic_lease(&lease), "{lease:?}");
        assert!(netd.published_nic().is_none());
        assert!(netd.dhcp_lease().is_none());
    }
}

#[test]
fn the_qemu_configuration_keeps_the_user_mode_address() {
    let netd = new_netd_with_nic();
    let config = netd.published_nic().unwrap();
    assert!(config.is_qemu_user());
    assert_eq!(config.addr, [10, 0, 2, 15]);
    assert_eq!(config.prefix, 24);
    assert_eq!(config.gateway, Some([10, 0, 2, 2]));

    let platform = NicConfig::platform(NIC_MAC, 1500);
    assert!(!platform.is_qemu_user());
    assert_eq!(platform.addr, [0; 4]);
    assert_eq!(platform.prefix, 0);
    assert_eq!(platform.gateway, None);
}

#[test]
fn release_markers_are_formatted_from_the_lease_and_reply() {
    let lease = test_lease(
        Ipv4Addr::new(192, 168, 1, 23),
        24,
        Some(Ipv4Addr::new(192, 168, 1, 1)),
        Ipv4Addr::new(192, 168, 1, 1),
    );
    assert_eq!(
        release_dhcp_line(&lease).as_deref(),
        Some("dhcp 192.168.1.23/24 192.168.1.1 192.168.1.1\n")
    );
    let no_dns = DhcpLease {
        dns: alloc::vec![],
        ..lease
    };
    assert_eq!(
        release_dhcp_line(&no_dns).as_deref(),
        Some("dhcp 192.168.1.23/24 192.168.1.1 none\n")
    );
    let no_router = DhcpLease {
        router: None,
        ..no_dns
    };
    assert_eq!(release_dhcp_line(&no_router), None);
    assert_eq!(
        release_ping_line(ReleasePing {
            target: [192, 168, 1, 1],
            rtt_ms: 3,
        }),
        "ping 192.168.1.1 tx=1 rx=1 rtt_ms=3\n"
    );
}

#[test]
fn only_the_nic_gateway_records_a_release_ping() {
    let mut netd = new_netd_with_nic();
    netd.record_release_ping(Interface::Nic, [10, 0, 2, 9], 4);
    assert_eq!(netd.take_release_ping(), None);
    netd.record_release_ping(Interface::Loopback, [10, 0, 2, 2], 4);
    assert_eq!(netd.take_release_ping(), None);
    netd.record_release_ping(Interface::Nic, [10, 0, 2, 2], 7);
    assert_eq!(
        netd.take_release_ping(),
        Some(ReleasePing {
            target: [10, 0, 2, 2],
            rtt_ms: 7,
        })
    );
    assert_eq!(netd.take_release_ping(), None);
}

#[test]
fn a_static_nic_is_published_without_dhcp() {
    let mut netd = new_netd();
    netd.attach_nic(
        Loopback::new(NIC_MAC),
        NicConfig::static_config(NIC_MAC, 1500, [10, 0, 2, 15], 24, Some([10, 0, 2, 2])),
        0,
    );
    netd.set_nic_dns(vec![[10, 0, 2, 3]]);
    let config = netd.published_nic().unwrap();
    assert_eq!(config.addr, [10, 0, 2, 15]);
    assert_eq!(config.prefix, 24);
    assert_eq!(config.gateway, Some([10, 0, 2, 2]));
    let lease = netd.dhcp_lease().unwrap();
    assert_eq!(lease.address.octets(), [10, 0, 2, 15]);
    assert_eq!(lease.prefix_len, 24);
    assert_eq!(lease.router.unwrap().octets(), [10, 0, 2, 2]);
    assert_eq!(lease.dns[0].octets(), [10, 0, 2, 3]);
    assert!(lease.renew_at_ms.is_none());
}

#[test]
fn a_qemu_fixed_nic_publishes_a_lease_for_resolvd() {
    // QEMU user-mode configures 10.0.2.15/24 immediately but is not NicAddressing::Static.
    // resolvd asks netd for a DHCP lease; this configuration must still answer with the
    // synthetic lease and the slirp DNS, not ENOENT.
    let mut netd = new_netd();
    netd.attach_nic(Loopback::new(NIC_MAC), NicConfig::qemu(NIC_MAC, 1500), 0);
    let lease = netd
        .dhcp_lease()
        .expect("QEMU fixed-address mode must publish a synthetic lease");
    assert_eq!(lease.address.octets(), [10, 0, 2, 15]);
    assert_eq!(lease.prefix_len, 24);
    assert_eq!(lease.router.unwrap().octets(), [10, 0, 2, 2]);
    assert_eq!(lease.dns[0].octets(), [10, 0, 2, 3]);
    assert!(lease.renew_at_ms.is_none());
}

#[test]
fn a_late_static_config_replaces_the_published_address() {
    let mut netd = new_netd_with_nic();
    netd.apply_static(&StaticConfig {
        addr: [192, 168, 7, 20],
        prefix: 25,
        gateway: Some([192, 168, 7, 1]),
        dns: vec![[192, 168, 7, 53]],
    });
    let config = netd.published_nic().unwrap();
    assert_eq!(config.addr, [192, 168, 7, 20]);
    assert_eq!(config.prefix, 25);
    assert_eq!(config.gateway, Some([192, 168, 7, 1]));
    let lease = netd.dhcp_lease().unwrap();
    assert_eq!(lease.dns[0].octets(), [192, 168, 7, 53]);
}

// ---- K244: live readiness query ---------------------------------------------------------

fn readiness(netd: &mut Netd<Loopback>, file: u64) -> Response {
    call_socket(netd, file, Call::Poll)
}

fn ready_bits(netd: &mut Netd<Loopback>, file: u64) -> i32 {
    match readiness(netd, file) {
        Response::Value(bits) => bits,
        other => panic!("unexpected readiness response: {other:?}"),
    }
}

fn readiness_retry(netd: &mut Netd<Loopback>, file: u64, bit: i32, now: &mut i64) -> i32 {
    for _ in 0..200 {
        poll(netd, now);
        let bits = ready_bits(netd, file);
        if bits & bit != 0 {
            return bits;
        }
    }
    panic!("readiness bit {bit:#x} did not appear");
}

#[test]
fn readiness_query_is_non_consuming_and_directional() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let id = accept(&mut netd, listener, &mut now);
    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();

    let idle = ready_bits(&mut netd, accepted);
    assert_eq!(idle & READY_WRITABLE, READY_WRITABLE);
    assert_eq!(idle & READY_READABLE, 0);
    assert_eq!(idle & READY_ERROR, 0);

    assert_eq!(netd.write(client, 0, b"hello").unwrap(), 5);
    let first = readiness_retry(&mut netd, accepted, READY_READABLE, &mut now);
    let second = ready_bits(&mut netd, accepted);
    assert_eq!(first, second, "a query does not consume the bytes");
    assert_eq!(first & READY_WRITABLE, READY_WRITABLE);

    let mut buffer = [0u8; 16];
    let read = read_retry(&mut netd, accepted, &mut buffer, &mut now);
    assert_eq!(&buffer[..read], b"hello");
    let drained = ready_bits(&mut netd, accepted);
    assert_eq!(drained & READY_READABLE, 0);
}

#[test]
fn readiness_shutdown_follows_freebsd_directions() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let id = accept(&mut netd, listener, &mut now);
    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();

    assert_eq!(
        ready_bits(&mut netd, accepted) & READY_WRITABLE,
        READY_WRITABLE
    );

    // Pending data makes the accepted side readable before either shutdown.
    assert_eq!(netd.write(client, 0, b"x"), Ok(1));
    let readable = readiness_retry(&mut netd, accepted, READY_READABLE, &mut now);
    assert_eq!(readable & READY_READABLE, READY_READABLE);
    assert_eq!(readable & READY_WRITABLE, READY_WRITABLE);

    // FreeBSD sowriteable: SHUT_WR sets SBS_CANTSENDMORE and POLLOUT stays set.
    assert_eq!(
        call_socket(
            &mut netd,
            accepted,
            Call::Shutdown {
                how: Shutdown::Write
            }
        ),
        Response::Ok
    );
    let write_shutdown = ready_bits(&mut netd, accepted);
    assert_eq!(
        write_shutdown & READY_WRITABLE,
        READY_WRITABLE,
        "FreeBSD SBS_CANTSENDMORE keeps POLLOUT"
    );
    assert_eq!(
        write_shutdown & READY_READABLE,
        READY_READABLE,
        "write shutdown must not clear pending read data"
    );
    assert_eq!(netd.write(accepted, 0, b"x"), Err(Errno::EPIPE));

    // FreeBSD soreadable: SHUT_RD does not clear the write side.
    assert_eq!(
        call_socket(
            &mut netd,
            accepted,
            Call::Shutdown {
                how: Shutdown::Read
            }
        ),
        Response::Ok
    );
    let read_shutdown = ready_bits(&mut netd, accepted);
    assert_eq!(
        read_shutdown & READY_WRITABLE,
        READY_WRITABLE,
        "read shutdown must not clear writable"
    );
    let mut buffer = [0u8; 4];
    assert_eq!(netd.read(accepted, 0, &mut buffer), Ok(0));

    // Full shutdown closes both file directions; the FreeBSD poll predicate remains writable.
    assert_eq!(
        call_socket(
            &mut netd,
            accepted,
            Call::Shutdown {
                how: Shutdown::Both
            }
        ),
        Response::Ok
    );
    assert_eq!(netd.read(accepted, 0, &mut buffer), Ok(0));
    assert_eq!(netd.write(accepted, 0, b"x"), Err(Errno::EPIPE));
    let both = ready_bits(&mut netd, accepted);
    assert_eq!(
        both & READY_WRITABLE,
        READY_WRITABLE,
        "FreeBSD SBS_CANTSENDMORE keeps POLLOUT"
    );
}

#[test]
fn listener_readiness_follows_the_accept_backlog() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    assert_eq!(ready_bits(&mut netd, listener) & READY_READABLE, 0);

    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let bits = readiness_retry(&mut netd, listener, READY_READABLE, &mut now);
    assert_eq!(bits & READY_WRITABLE, 0);

    let _id = accept(&mut netd, listener, &mut now);
    assert_eq!(ready_bits(&mut netd, listener) & READY_READABLE, 0);
}

#[test]
fn wildcard_listener_serves_loopback_and_nic() {
    let mut netd = new_netd_with_nic();
    let mut now = 0i64;
    // A 0.0.0.0 bind selects the NIC interface at open time. The listener must still serve the
    // loopback core: a client to 127.0.0.1 has to become readable and accept.
    let listener = netd.open("net:tcp/0.0.0.0:7010").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );

    let loopback = Endpoint {
        ip: [127, 0, 0, 1],
        port: 7010,
    };
    let loop_client = netd.open("net:tcp/127.0.0.1:7010").unwrap();
    assert_eq!(
        call_socket(&mut netd, loop_client, Call::Connect(loopback)),
        Response::Ok
    );
    let bits = readiness_retry(&mut netd, listener, READY_READABLE, &mut now);
    assert_eq!(bits & READY_WRITABLE, 0);
    let id = accept(&mut netd, listener, &mut now);
    let loop_accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();
    let loop_client_local = call_socket(&mut netd, loop_client, Call::GetSockName);
    assert_eq!(
        call_socket(&mut netd, loop_accepted, Call::GetPeerName),
        loop_client_local,
        "the wildcard loopback accepted peer must be the client address"
    );
    assert_eq!(netd.write(loop_client, 0, b"loop").unwrap(), 4);
    let mut buffer = [0u8; 8];
    let read = read_retry(&mut netd, loop_accepted, &mut buffer, &mut now);
    assert_eq!(&buffer[..read], b"loop");

    // The same wildcard listener keeps serving the NIC path.
    let nic = Endpoint {
        ip: [10, 0, 2, 15],
        port: 7010,
    };
    let nic_client = netd.open("net:tcp/10.0.2.15:7010").unwrap();
    assert_eq!(
        call_socket(&mut netd, nic_client, Call::Connect(nic)),
        Response::Ok
    );
    let id = accept(&mut netd, listener, &mut now);
    let nic_accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();
    let nic_client_local = call_socket(&mut netd, nic_client, Call::GetSockName);
    assert_eq!(
        call_socket(&mut netd, nic_accepted, Call::GetPeerName),
        nic_client_local,
        "the accepted NIC socket must report the client's address and port"
    );
    assert_eq!(netd.write(nic_client, 0, b"nic").unwrap(), 3);
    let read = read_retry(&mut netd, nic_accepted, &mut buffer, &mut now);
    assert_eq!(&buffer[..read], b"nic");

    netd.close(nic_accepted).unwrap();
    netd.close(loop_accepted).unwrap();
    netd.close(nic_client).unwrap();
    netd.close(loop_client).unwrap();
    netd.close(listener).unwrap();
    assert_eq!(netd.socket_files(), 0);
}

#[test]
fn readiness_reports_a_peer_half_close_as_readable_not_hup() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let id = accept(&mut netd, listener, &mut now);
    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();

    assert_eq!(
        call_socket(
            &mut netd,
            client,
            Call::Shutdown {
                how: Shutdown::Write
            }
        ),
        Response::Ok
    );
    let bits = readiness_retry(&mut netd, accepted, READY_READABLE, &mut now);
    assert_eq!(bits & READY_HUP, 0, "a half-close is not a full hangup");

    let mut buffer = [0u8; 4];
    assert_eq!(netd.read(accepted, 0, &mut buffer), Ok(0));
    let after = ready_bits(&mut netd, accepted);
    assert_eq!(after & READY_READABLE, READY_READABLE);
    assert_eq!(after & READY_HUP, 0);
}

#[test]
fn readiness_has_defined_errors_for_invalid_and_non_socket_files() {
    let mut netd = new_netd();

    assert_eq!(netd.readiness(999), Err(Errno::EBADF));

    let control = netd.open("net:").unwrap();
    assert_eq!(netd.readiness(control), Err(Errno::EINVAL));

    let unbound = netd.open("net:tcp/127.0.0.1:7100").unwrap();
    assert_eq!(netd.readiness(unbound), Err(Errno::ENOTCONN));
}

// ---- K240 edge/doorbell regressions, kept green by K244 -------------------------------

fn poll_events(netd: &mut Netd<Loopback>, now: &mut i64) -> Vec<raven_netcore::Event> {
    *now = now.saturating_add(1);
    netd.poll(*now)
        .into_iter()
        .map(|ready| ready.event)
        .collect()
}

fn established_pair(netd: &mut Netd<Loopback>, now: &mut i64) -> (u64, u64, u64) {
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let id = accept(netd, listener, now);
    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();
    (listener, client, accepted)
}

#[test]
fn data_arriving_in_the_pass_after_a_drain_still_raises_readable() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let (_listener, client, accepted) = established_pair(&mut netd, &mut now);
    let socket = netd.socket_id(accepted).expect("accepted socket");

    assert_eq!(netd.write(client, 0, b"a").unwrap(), 1);
    let mut seen = false;
    for _ in 0..50 {
        seen |= poll_events(&mut netd, &mut now).contains(&raven_netcore::Event::Readable(socket));
        if seen {
            break;
        }
    }
    assert!(seen, "the first byte raises Readable");

    // The application drains the queue between two passes, and the next byte arrives in the very
    // next pass. The edge baseline must follow the drain or this raises no event.
    let mut buffer = [0u8; 8];
    assert_eq!(netd.read(accepted, 0, &mut buffer).unwrap(), 1);
    assert_eq!(netd.write(client, 0, b"b").unwrap(), 1);
    let mut seen = false;
    for _ in 0..50 {
        seen |= poll_events(&mut netd, &mut now).contains(&raven_netcore::Event::Readable(socket));
        if seen {
            break;
        }
    }
    assert!(seen, "a refill right after a drain raises Readable again");
}

#[test]
fn a_connection_completing_right_after_an_accept_still_raises_accepted() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    let listener_socket = netd.socket_id(listener).expect("listener socket");
    let first = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, first, Call::Connect(SERVER)),
        Response::Ok
    );
    let mut announced = false;
    for _ in 0..50 {
        announced |= poll_events(&mut netd, &mut now)
            .iter()
            .any(|event| matches!(event, raven_netcore::Event::Accepted { .. }));
        if announced {
            break;
        }
    }
    assert!(announced, "the first connection is announced");

    let second = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, second, Call::Connect(SERVER)),
        Response::Ok
    );
    assert!(matches!(
        call_socket(&mut netd, listener, Call::Accept),
        Response::Socket { .. }
    ));
    let mut announced_again = false;
    for _ in 0..50 {
        announced_again |= poll_events(&mut netd, &mut now).iter().any(|event| {
            matches!(event, raven_netcore::Event::Accepted { listener, .. } if *listener == listener_socket)
        });
        if announced_again {
            break;
        }
    }
    assert!(
        announced_again,
        "the connection that completed after the accept is announced too"
    );
}

#[test]
fn live_readiness_follows_backlog_data_and_end_of_stream() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    assert!(!netd.socket_readiness(listener).unwrap().readable);

    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    for _ in 0..50 {
        poll(&mut netd, &mut now);
        if netd.socket_readiness(listener).unwrap().readable {
            break;
        }
    }
    assert!(netd.socket_readiness(listener).unwrap().readable);
    let id = accept(&mut netd, listener, &mut now);
    assert!(!netd.socket_readiness(listener).unwrap().readable);

    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();
    assert_eq!(netd.write(client, 0, b"hello").unwrap(), 5);
    for _ in 0..50 {
        poll(&mut netd, &mut now);
        if netd.socket_readiness(accepted).unwrap().readable {
            break;
        }
    }
    assert!(netd.socket_readiness(accepted).unwrap().readable);
    let mut buffer = [0u8; 16];
    assert_eq!(read_retry(&mut netd, accepted, &mut buffer, &mut now), 5);
    assert!(!netd.socket_readiness(accepted).unwrap().readable);

    assert_eq!(
        call_socket(
            &mut netd,
            client,
            Call::Shutdown {
                how: Shutdown::Write
            }
        ),
        Response::Ok
    );
    for _ in 0..50 {
        poll(&mut netd, &mut now);
        if netd.socket_readiness(accepted).unwrap().readable {
            break;
        }
    }
    assert!(netd.socket_readiness(accepted).unwrap().readable);
    assert_eq!(netd.read(accepted, 0, &mut buffer).unwrap(), 0);
    assert!(netd.socket_readiness(accepted).unwrap().readable);
    assert!(!netd.socket_readiness(accepted).unwrap().hup);
}

#[test]
fn file_references_keep_the_socket_alive_until_the_last_close() {
    let mut netd = new_netd();
    let mut now = 0i64;
    let listener = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, listener, Call::Listen { backlog: 4 }),
        Response::Ok
    );
    let client = netd.open("net:tcp/127.0.0.1:7000").unwrap();
    assert_eq!(
        call_socket(&mut netd, client, Call::Connect(SERVER)),
        Response::Ok
    );
    let id = accept(&mut netd, listener, &mut now);
    let accepted = netd.open(&alloc::format!("net:accept/{id}")).unwrap();
    let socket = netd.socket_id(accepted).expect("accepted socket");
    let files = netd.socket_files();

    // A fork child takes one more reference. The parent's close drops only its own.
    assert_eq!(netd.dup(accepted), Ok(accepted));
    assert_eq!(netd.close(accepted), Ok(false));
    assert_eq!(netd.socket_id(accepted), Some(socket));
    assert_eq!(netd.socket_files(), files);

    assert_eq!(netd.write(client, 0, b"keep").unwrap(), 4);
    let mut buffer = [0u8; 8];
    let read = read_retry(&mut netd, accepted, &mut buffer, &mut now);
    assert_eq!(&buffer[..read], b"keep");

    assert_eq!(netd.close(accepted), Ok(true));
    assert_eq!(netd.socket_id(accepted), None);
    assert_eq!(netd.socket_files(), files - 1);
    assert_eq!(netd.close(accepted), Err(Errno::EBADF));
}

#[test]
fn a_filtered_server_drains_the_shared_filter_log() {
    let policy = crate::FilterPolicy::from_text("set default pass\npass in on lo all\n").unwrap();
    policy
        .log()
        .borrow_mut()
        .push(alloc::string::String::from("netd: pf test"));
    let mut netd = Netd::with_filter(Loopback::new(NIC_MAC), NIC_MAC, 0, Some(policy));
    assert_eq!(
        netd.take_filter_log(),
        alloc::vec![alloc::string::String::from("netd: pf test")]
    );
    assert!(netd.take_filter_log().is_empty());
}
