// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the address, hint and result records.

use super::*;
use alloc::string::String;

fn v4(a: [u8; 4]) -> IpAddr {
    IpAddr::V4(a)
}

fn v6(last: u8) -> IpAddr {
    let mut bytes = [0u8; 16];
    bytes[15] = last;
    IpAddr::V6(bytes)
}

#[test]
fn family_af_numbers() {
    assert_eq!(Family::V4.af(), 2);
    assert_eq!(Family::V6.af(), 10);
}

#[test]
fn family_from_af_accepts_only_linux_numbers() {
    assert_eq!(Family::from_af(2), Some(Family::V4));
    assert_eq!(Family::from_af(10), Some(Family::V6));
    assert_eq!(Family::from_af(0), None);
    assert_eq!(Family::from_af(24), None);
    assert_eq!(Family::from_af(-1), None);
}

#[test]
fn family_other_flips() {
    assert_eq!(Family::V4.other(), Family::V6);
    assert_eq!(Family::V6.other(), Family::V4);
}

#[test]
fn family_sizes() {
    assert_eq!(Family::V4.size(), 4);
    assert_eq!(Family::V6.size(), 16);
}

#[test]
fn ipv4_bytes_are_four() {
    assert_eq!(v4([127, 0, 0, 1]).as_bytes(), &[127, 0, 0, 1]);
    assert_eq!(v4([1, 2, 3, 4]).family(), Family::V4);
}

#[test]
fn ipv4_octets16_are_v4_mapped() {
    assert_eq!(
        v4([192, 0, 2, 1]).octets16(),
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 0, 2, 1]
    );
}

#[test]
fn ipv4_from_octets16_takes_the_tail() {
    let octets = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 10, 0, 0, 7];
    assert_eq!(
        IpAddr::from_octets16(Family::V4, &octets),
        v4([10, 0, 0, 7])
    );
}

#[test]
fn ipv6_from_octets16_keeps_all_bytes() {
    let mut octets = [0u8; 16];
    octets[0] = 0x20;
    octets[15] = 1;
    assert_eq!(
        IpAddr::from_octets16(Family::V6, &octets),
        IpAddr::V6(octets)
    );
}

#[test]
fn unspecified_v4() {
    assert!(v4([0, 0, 0, 0]).is_unspecified());
    assert!(!v4([0, 0, 0, 1]).is_unspecified());
}

#[test]
fn unspecified_v6() {
    assert!(IpAddr::V6([0u8; 16]).is_unspecified());
    assert!(!v6(1).is_unspecified());
}

#[test]
fn loopback_v4() {
    assert!(v4([127, 0, 0, 1]).is_loopback());
    assert!(v4([127, 255, 255, 254]).is_loopback());
    assert!(!v4([128, 0, 0, 1]).is_loopback());
}

#[test]
fn loopback_v6() {
    assert!(v6(1).is_loopback());
    assert!(!v6(2).is_loopback());
}

#[test]
fn link_local_v6() {
    let mut bytes = [0u8; 16];
    bytes[0] = 0xfe;
    bytes[1] = 0x80;
    assert!(IpAddr::V6(bytes).is_link_local());
    bytes[1] = 0x7f;
    assert!(!IpAddr::V6(bytes).is_link_local());
    assert!(!v4([169, 254, 0, 1]).is_link_local());
}

#[test]
fn link_local_multicast_v6() {
    let mut bytes = [0u8; 16];
    bytes[0] = 0xff;
    bytes[1] = 0x02;
    assert!(IpAddr::V6(bytes).is_mc_link_local());
    bytes[1] = 0x05;
    assert!(!IpAddr::V6(bytes).is_mc_link_local());
}

#[test]
fn a_v4_address_maps_and_unmaps() {
    let mapped = v4([203, 0, 113, 9]).to_v4_mapped();
    assert_eq!(mapped.family(), Family::V6);
    assert_eq!(mapped.as_v4_mapped(), Some([203, 0, 113, 9]));
    assert_eq!(v4([203, 0, 113, 9]).as_v4_mapped(), Some([203, 0, 113, 9]));
}

#[test]
fn a_real_v6_address_is_not_v4_mapped() {
    let mut bytes = [0u8; 16];
    bytes[0] = 0x20;
    bytes[1] = 0x01;
    assert_eq!(IpAddr::V6(bytes).as_v4_mapped(), None);
}

#[test]
fn sockaddr_constructors() {
    let plain = SockAddr::new(v4([127, 0, 0, 1]), 53);
    assert_eq!(plain.scope, 0);
    assert_eq!(plain.family(), Family::V4);
    let scoped = SockAddr::with_scope(v6(1), 80, 7);
    assert_eq!(scoped.scope, 7);
    assert_eq!(scoped.family(), Family::V6);
}

#[test]
fn hints_default_is_zero() {
    let hints = Hints::default();
    assert_eq!(
        (hints.flags, hints.family, hints.socktype, hints.protocol),
        (0, 0, 0, 0)
    );
}

#[test]
fn hints_family_validation() {
    assert!(Hints::default().family_valid());
    assert!(
        Hints {
            family: 2,
            ..Hints::default()
        }
        .family_valid()
    );
    assert!(
        Hints {
            family: 10,
            ..Hints::default()
        }
        .family_valid()
    );
    assert!(
        !Hints {
            family: 24,
            ..Hints::default()
        }
        .family_valid()
    );
}

#[test]
fn hints_socktype_names() {
    assert_eq!(
        Hints {
            socktype: 1,
            ..Hints::default()
        }
        .socktype_name(),
        Some("stream")
    );
    assert_eq!(
        Hints {
            socktype: 2,
            ..Hints::default()
        }
        .socktype_name(),
        Some("dgram")
    );
    assert_eq!(
        Hints {
            socktype: 3,
            ..Hints::default()
        }
        .socktype_name(),
        None
    );
}

#[test]
fn udp_interest_targets_the_shared_socket() {
    let interest = Interest::udp(UdpSock(9));
    assert_eq!(interest.kind, InterestKind::Udp(UdpSock(9)));
    assert_eq!(interest.query, usize::MAX);
    assert!(!interest.ready_r && !interest.ready_w);
}

#[test]
fn tcp_interest_carries_the_query_and_direction() {
    let interest = Interest::tcp(TcpSock(4), 2, true);
    assert_eq!(interest.kind, InterestKind::Tcp(TcpSock(4)));
    assert_eq!(interest.query, 2);
    assert!(interest.write);
}

#[test]
fn recv_record_carries_the_source() {
    let received = Recv {
        len: 12,
        from: SockAddr::new(v6(1), 53),
        truncated: true,
    };
    assert_eq!(received.len, 12);
    assert!(received.truncated);
    assert_eq!(received.from.family(), Family::V6);
}

#[test]
fn addrinfo_record_holds_a_canonical_name() {
    let record = AddrInfoRec {
        flags: 2,
        family: 2,
        socktype: 1,
        protocol: 6,
        addr: SockAddr::new(v4([127, 0, 0, 1]), 80),
        canonname: Some(String::from("localhost")),
    };
    assert_eq!(record.canonname.as_deref(), Some("localhost"));
    assert_eq!(record.addr.port, 80);
}
