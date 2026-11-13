// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the netd file table, loopback TCP/UDP and interface control.

use super::*;
use crate::Loopback;
use raven_proto_net::{IfaceRequest, IfaceResponse, Route, Shutdown, SockOpt};

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
    netd.attach_nic(
        Loopback::new(NIC_MAC),
        NicConfig {
            mac: NIC_MAC,
            mtu: 1500,
            addr: [10, 0, 2, 15],
            prefix: 24,
            gateway: Some([10, 0, 2, 2]),
            dhcp: false,
        },
        0,
    );
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
