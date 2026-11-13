// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sockaddr` tests.

use super::*;
use crate::ErrnoExt;

#[test]
fn parse_ipv4_vector() {
    let bytes = [2, 0, 0, 80, 10, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0];
    let parsed = parse_sockaddr(&bytes).expect("parse");
    assert_eq!(
        parsed,
        SockAddr::In {
            ip: [10, 0, 0, 2],
            port: 80
        }
    );
}

#[test]
fn write_ipv4_vector_and_padding() {
    let mut out = [0xffu8; 16];
    let full = write_sockaddr(
        &SockAddr::In {
            ip: [10, 0, 0, 2],
            port: 80,
        },
        &mut out,
    );
    assert_eq!(full, 16);
    assert_eq!(out, [2, 0, 0, 80, 10, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn port_byte_order() {
    let mut out = [0u8; 16];
    write_sockaddr(
        &SockAddr::In {
            ip: [1, 2, 3, 4],
            port: 0x1234,
        },
        &mut out,
    );
    assert_eq!(&out[2..4], &[0x12, 0x34]);
}

#[test]
fn short_ipv4_is_invalid() {
    let mut bytes = [0u8; 15];
    bytes[0] = 2;
    assert_eq!(parse_sockaddr(&bytes), Err(Errno::EINVAL));
}

#[test]
fn wrong_family_is_eafnosupport() {
    let mut bytes = [0u8; 16];
    bytes[0] = 99;
    assert_eq!(parse_sockaddr(&bytes), Err(Errno::EAFNOSUPPORT));
}

#[test]
fn truncation_rule() {
    let mut out = [0u8; 4];
    let full = write_sockaddr(
        &SockAddr::In {
            ip: [10, 0, 0, 2],
            port: 80,
        },
        &mut out,
    );
    assert_eq!(full, 16);
    assert_eq!(out, [2, 0, 0, 80]);
}

#[test]
fn parse_ipv6_and_write() {
    let mut bytes = [0u8; 28];
    bytes[0] = 10;
    bytes[2] = 0;
    bytes[3] = 53;
    bytes[8] = 0x20;
    bytes[9] = 0x01;
    bytes[10] = 0x0d;
    bytes[11] = 0xb8;
    let parsed = parse_sockaddr(&bytes).expect("parse v6");
    assert_eq!(parsed.family(), 10);
    assert_eq!(parsed.port(), 53);
    let mut out = [0u8; 28];
    assert_eq!(write_sockaddr(&parsed, &mut out), 28);
    assert_eq!(out, bytes);
}

#[test]
fn unix_path_roundtrip() {
    let mut bytes = [0u8; 110];
    bytes[0] = 1;
    bytes[2..6].copy_from_slice(b"/tmp");
    let parsed = parse_sockaddr(&bytes).expect("parse unix");
    assert_eq!(parsed.unix_path(), Some(&b"/tmp"[..]));
    let mut out = [0u8; 110];
    assert_eq!(write_sockaddr(&parsed, &mut out), 110);
    assert_eq!(&out[2..6], b"/tmp");
}

#[test]
fn endpoint_roundtrip() {
    let addr = SockAddr::In {
        ip: [192, 0, 2, 7],
        port: 53,
    };
    let endpoint = addr.to_endpoint().expect("v4 endpoint");
    assert_eq!(endpoint.ip, [192, 0, 2, 7]);
    assert_eq!(endpoint.port, 53);
    assert_eq!(SockAddr::from_endpoint(endpoint), addr);
}

#[test]
fn ipv6_has_no_wire_endpoint() {
    let addr = SockAddr::In6 {
        ip: [0; 16],
        port: 1,
        scope: 0,
        flowinfo: 0,
    };
    assert!(addr.to_endpoint().is_none());
    assert!(addr.ipv4().is_none());
}
