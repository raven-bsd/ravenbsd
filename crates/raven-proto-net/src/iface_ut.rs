// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `net:` control wire format.

use super::*;

fn encode(request: IfaceRequest) -> Vec<u8> {
    let mut buffer = [0u8; 128];
    let length = request.encode(&mut buffer).unwrap();
    buffer[..length].to_vec()
}

fn encode_response(response: IfaceResponse) -> Vec<u8> {
    let mut buffer = [0u8; 128];
    let length = response.encode(&mut buffer).unwrap();
    buffer[..length].to_vec()
}

#[test]
fn list_and_arp_list_golden_bytes() {
    assert_eq!(encode(IfaceRequest::List), [0x01, 0x00, 0x01, 0x00]);
    assert_eq!(encode(IfaceRequest::ArpList), [0x01, 0x00, 0x07, 0x00]);
}

#[test]
fn get_golden_bytes() {
    assert_eq!(
        encode(IfaceRequest::Get {
            name: IfName::new("eth0").unwrap()
        }),
        [0x01, 0x00, 0x02, 0x00, 0x04, b'e', b't', b'h', b'0']
    );
}

#[test]
fn set_address_golden_bytes() {
    assert_eq!(
        encode(IfaceRequest::SetAddress {
            name: IfName::new("lo").unwrap(),
            addr: [127, 0, 0, 1],
            prefix: 8
        }),
        [0x01, 0x00, 0x03, 0x00, 0x02, b'l', b'o', 127, 0, 0, 1, 8]
    );
}

#[test]
fn route_golden_bytes() {
    assert_eq!(
        encode(IfaceRequest::RouteAdd {
            route: Route {
                dest: [0, 0, 0, 0],
                prefix: 0,
                gateway: None
            }
        }),
        [0x01, 0x00, 0x05, 0x00, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn requests_roundtrip() {
    let name = IfName::new("virtio0").unwrap();
    for request in [
        IfaceRequest::List,
        IfaceRequest::Get { name },
        IfaceRequest::SetAddress {
            name,
            addr: [10, 0, 0, 1],
            prefix: 24,
        },
        IfaceRequest::SetState { name, up: false },
        IfaceRequest::RouteAdd {
            route: Route {
                dest: [10, 0, 0, 0],
                prefix: 8,
                gateway: Some([10, 0, 0, 254]),
            },
        },
        IfaceRequest::RouteDel {
            dest: [10, 0, 0, 0],
            prefix: 8,
        },
        IfaceRequest::ArpList,
    ] {
        let bytes = encode(request);
        assert_eq!(IfaceRequest::decode(&bytes), Ok(request), "{request:?}");
    }
}

#[test]
fn interface_response_golden_bytes() {
    let info = IfaceInfo {
        name: IfName::new("lo").unwrap(),
        addr: [127, 0, 0, 1],
        prefix: 8,
        flags: IF_UP | IF_LINK_UP | IF_LOOPBACK,
        mtu: 65_535,
        mac: [0, 0, 0, 0, 0, 0],
    };
    assert_eq!(
        encode_response(IfaceResponse::Iface(info)),
        [
            0x01, 0x00, 0x00, 0x00, 0x02, 0x00, // header
            0x02, b'l', b'o', // name
            127, 0, 0, 1, // addr
            8, 0x07, 0xff, 0xff, // prefix, flags, mtu
            0, 0, 0, 0, 0, 0, // mac
        ]
    );
}

#[test]
fn responses_roundtrip() {
    let name = IfName::new("eth0").unwrap();
    for response in [
        IfaceResponse::Ok,
        IfaceResponse::Count { count: 3 },
        IfaceResponse::Iface(IfaceInfo {
            name,
            addr: [10, 0, 0, 1],
            prefix: 24,
            flags: IF_UP | IF_LINK_UP,
            mtu: 1500,
            mac: [2, 0, 0, 0, 0, 1],
        }),
        IfaceResponse::Route(Route {
            dest: [0, 0, 0, 0],
            prefix: 0,
            gateway: Some([10, 0, 0, 1]),
        }),
        IfaceResponse::Arp(ArpEntry {
            ip: [10, 0, 0, 2],
            mac: [2, 0, 0, 0, 0, 2],
            flags: ARP_COMPLETE,
        }),
        IfaceResponse::Err(Errno::ENODEV),
    ] {
        let bytes = encode_response(response.clone());
        assert_eq!(
            IfaceResponse::decode(&bytes),
            Ok(response.clone()),
            "{response:?}"
        );
    }
}

#[test]
fn invalid_names_are_rejected() {
    assert!(IfName::new("").is_none());
    assert!(IfName::new("this-name-is-too-long").is_none());
    assert!(IfName::new("eth 0").is_none());
    assert!(IfName::new("eth/0").is_none());
    assert_eq!(IfName::new("eth0").unwrap().as_str(), "eth0");
    assert!(IfName::new("a.b-c_9").is_some());
}

#[test]
fn invalid_prefixes_are_rejected() {
    let name = IfName::new("eth0").unwrap();
    let mut buffer = [0u8; 64];
    assert_eq!(
        IfaceRequest::SetAddress {
            name,
            addr: [1, 2, 3, 4],
            prefix: 33
        }
        .encode(&mut buffer),
        Err(WireError::Invalid)
    );
}

#[test]
fn truncation_and_trailing_bytes_are_rejected() {
    let bytes = encode(IfaceRequest::SetAddress {
        name: IfName::new("eth0").unwrap(),
        addr: [10, 0, 0, 2],
        prefix: 24,
    });
    for length in 0..bytes.len() {
        assert!(IfaceRequest::decode(&bytes[..length]).is_err(), "{length}");
    }
    let mut bytes = encode(IfaceRequest::List);
    bytes.push(0);
    assert_eq!(IfaceRequest::decode(&bytes), Err(DecodeError::trailing()));
}

#[test]
fn error_status_must_be_nonzero() {
    let bytes = [0x01, 0x00, 0x00, 0x00, 0x05, 0x00];
    assert_eq!(IfaceResponse::decode(&bytes), Err(DecodeError::malformed()));
}

#[test]
fn new_request_golden_bytes() {
    assert_eq!(encode(IfaceRequest::RouteList), [0x01, 0x00, 0x08, 0x00]);
    let lo = IfName::new("lo").unwrap();
    assert_eq!(
        encode(IfaceRequest::RouteAddOn {
            name: lo,
            route: Route {
                dest: [198, 51, 100, 0],
                prefix: 24,
                gateway: None,
            },
        }),
        [
            0x01, 0x00, 0x09, 0x00, 0x02, b'l', b'o', 198, 51, 100, 0, 24, 0
        ]
    );
    assert_eq!(
        encode(IfaceRequest::RouteDelOn {
            name: lo,
            dest: [198, 51, 100, 0],
            prefix: 24,
        }),
        [
            0x01, 0x00, 0x0a, 0x00, 0x02, b'l', b'o', 198, 51, 100, 0, 24
        ]
    );
    assert_eq!(
        encode(IfaceRequest::DhcpLease {
            name: IfName::new("eth0").unwrap(),
        }),
        [0x01, 0x00, 0x0b, 0x00, 0x04, b'e', b't', b'h', b'0']
    );
    assert_eq!(encode(IfaceRequest::SocketList), [0x01, 0x00, 0x0c, 0x00]);
    assert_eq!(
        encode(IfaceRequest::Counters {
            name: IfName::new("lo").unwrap(),
        }),
        [0x01, 0x00, 0x0d, 0x00, 0x02, b'l', b'o']
    );
}

#[test]
fn route_info_golden_bytes() {
    let response = IfaceResponse::RouteInfo(RouteInfo {
        dest: [198, 51, 100, 0],
        prefix: 24,
        flags: ROUTE_UP,
        iface: IfName::new("lo").unwrap(),
        gateway: None,
    });
    assert_eq!(
        encode_response(response),
        [
            0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 198, 51, 100, 0, 24, 0x01, 0x02, b'l', b'o'
        ]
    );
}

#[test]
fn dhcp_lease_golden_bytes() {
    let response = IfaceResponse::DhcpLease(DhcpLeaseInfo {
        addr: [10, 0, 2, 15],
        prefix: 24,
        router: Some([10, 0, 2, 2]),
        dns: vec![[10, 0, 2, 3]],
        renew_ms: DHCP_TIMER_UNKNOWN,
        rebind_ms: DHCP_TIMER_UNKNOWN,
        expires_ms: DHCP_TIMER_UNKNOWN,
    });
    assert_eq!(
        encode_response(response),
        [
            0x01, 0x00, 0x00, 0x00, 0x07, 0x00, 10, 0, 2, 15, 24, 1, 10, 0, 2, 2, 1, 10, 0, 2, 3,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff
        ]
    );
}

#[test]
fn socket_entry_golden_bytes() {
    let response = IfaceResponse::Socket(SocketEntry {
        id: 7,
        transport: SOCKET_TCP,
        state: SOCKET_STATE_ESTABLISHED,
        iface: IfName::new("lo").unwrap(),
        local: [127, 0, 0, 1],
        local_port: 7000,
        remote: Some([10, 0, 2, 2]),
        remote_port: 80,
        rx_queue: 1,
        tx_queue: 2,
    });
    assert_eq!(
        encode_response(response),
        [
            0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x07, 0x00, 0x00, 0x00, 1, 4, 0x02, b'l', b'o',
            127, 0, 0, 1, 0x58, 0x1b, 1, 10, 0, 2, 2, 0x50, 0x00, 1, 0, 0, 0, 2, 0, 0, 0
        ]
    );
}

#[test]
fn counter_info_roundtrips() {
    let counters = CounterInfo {
        rx_packets: 1,
        tx_packets: 2,
        rx_bytes: 3,
        tx_bytes: 4,
        rx_errors: 5,
        tx_errors: 6,
        rx_drops: 7,
        tx_drops: 8,
    };
    let bytes = encode_response(IfaceResponse::Counters(counters));
    assert_eq!(
        IfaceResponse::decode(&bytes),
        Ok(IfaceResponse::Counters(counters))
    );
}

#[test]
fn new_records_reject_invalid_fields() {
    let bad_route = IfaceResponse::RouteInfo(RouteInfo {
        dest: [0; 4],
        prefix: 24,
        flags: 0x80,
        iface: IfName::new("lo").unwrap(),
        gateway: None,
    });
    let mut buffer = [0u8; 128];
    assert!(bad_route.encode(&mut buffer).is_err());

    let bad_lease = DhcpLeaseInfo {
        addr: [10, 0, 2, 15],
        prefix: 24,
        router: None,
        dns: vec![[1, 1, 1, 1]; MAX_DHCP_DNS + 1],
        renew_ms: 0,
        rebind_ms: 0,
        expires_ms: 0,
    };
    assert!(
        IfaceResponse::DhcpLease(bad_lease)
            .encode(&mut buffer)
            .is_err()
    );
}
