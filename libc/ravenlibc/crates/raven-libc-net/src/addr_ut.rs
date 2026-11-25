// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sockaddr` tests.

use super::*;
use crate::ErrnoExt;

const AF_INET6: i32 = 28;
const AF_UNIX: i32 = 1;

#[test]
fn parse_ipv4_vector() {
    // FreeBSD `struct sockaddr_in`: sa_len at 0, family at 1, port at 2, address at 4.
    let bytes = [16, 2, 0, 80, 10, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0];
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
fn linux_family_word_is_not_a_freebsd_address() {
    // Regression for S174's offset-0 assertion: the old Linux two-byte family word
    // (family 2 = AF_INET at offset 0) must not be accepted as AF_INET.
    let mut bytes = [0u8; 16];
    bytes[0] = 2;
    assert_eq!(parse_sockaddr(&bytes), Err(Errno::EAFNOSUPPORT));
}

#[test]
fn ipv4_length_must_match_the_record() {
    let short = [15u8, 2, 0, 80, 10, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(parse_sockaddr(&short), Err(Errno::EINVAL));
    let long = [17u8, 2, 0, 80, 10, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(parse_sockaddr(&long), Err(Errno::EINVAL));
    let too_long = [0u8; 256];
    assert_eq!(parse_sockaddr(&too_long), Err(Errno::ENAMETOOLONG));
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
    assert_eq!(out, [16, 2, 0, 80, 10, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
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
    bytes[1] = 2;
    assert_eq!(parse_sockaddr(&bytes), Err(Errno::EINVAL));
}

#[test]
fn wrong_family_is_eafnosupport() {
    let mut bytes = [0u8; 16];
    bytes[1] = 99;
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
    assert_eq!(out, [16, 2, 0, 80]);
}

#[test]
fn parse_ipv6_and_write() {
    let mut bytes = [0u8; 28];
    bytes[0] = 28;
    bytes[1] = AF_INET6 as u8;
    bytes[2] = 0;
    bytes[3] = 53;
    bytes[8] = 0x20;
    bytes[9] = 0x01;
    bytes[10] = 0x0d;
    bytes[11] = 0xb8;
    let parsed = parse_sockaddr(&bytes).expect("parse v6");
    assert_eq!(parsed.family(), AF_INET6 as u16);
    assert_eq!(parsed.port(), 53);
    let mut out = [0u8; 28];
    assert_eq!(write_sockaddr(&parsed, &mut out), 28);
    assert_eq!(out, bytes);
}

#[test]
fn unix_path_roundtrip() {
    let mut bytes = [0u8; 6];
    bytes[0] = 6;
    bytes[1] = AF_UNIX as u8;
    bytes[2..6].copy_from_slice(b"/tmp");
    let parsed = parse_sockaddr(&bytes).expect("parse unix");
    assert_eq!(parsed.unix_path(), Some(&b"/tmp"[..]));
    let mut out = [0u8; 6];
    assert_eq!(write_sockaddr(&parsed, &mut out), 6);
    assert_eq!(out, bytes);
}

#[test]
fn unix_maximum_path_and_oversize() {
    let mut bytes = [0u8; 106];
    bytes[0] = 106;
    bytes[1] = AF_UNIX as u8;
    bytes[2..].fill(b'x');
    let parsed = parse_sockaddr(&bytes).expect("parse unix");
    assert_eq!(parsed.unix_path().map(<[u8]>::len), Some(104));

    let mut out = [0u8; 106];
    assert_eq!(write_sockaddr(&parsed, &mut out), 106);
    assert_eq!(out, bytes);

    let mut oversize = [0u8; 107];
    oversize[1] = AF_UNIX as u8;
    assert_eq!(parse_sockaddr(&oversize), Err(Errno::EINVAL));
}

#[test]
fn generated_socket_records_are_freebsd_shaped() {
    use core::mem::{align_of, offset_of, size_of};
    use raven_libc_abi::structs::{
        Addrinfo, Cmsghdr, Msghdr, Sockaddr, SockaddrIn, SockaddrIn6, SockaddrStorage, SockaddrUn,
    };

    assert_eq!((size_of::<Sockaddr>(), align_of::<Sockaddr>()), (16, 1));
    assert_eq!(offset_of!(Sockaddr, sa_len), 0);
    assert_eq!(offset_of!(Sockaddr, sa_family), 1);
    assert_eq!(offset_of!(Sockaddr, sa_data), 2);

    assert_eq!((size_of::<SockaddrIn>(), align_of::<SockaddrIn>()), (16, 4));
    assert_eq!(offset_of!(SockaddrIn, sin_len), 0);
    assert_eq!(offset_of!(SockaddrIn, sin_family), 1);
    assert_eq!(offset_of!(SockaddrIn, sin_port), 2);
    assert_eq!(offset_of!(SockaddrIn, sin_addr), 4);

    assert_eq!(
        (size_of::<SockaddrIn6>(), align_of::<SockaddrIn6>()),
        (28, 4)
    );
    assert_eq!(offset_of!(SockaddrIn6, sin6_len), 0);
    assert_eq!(offset_of!(SockaddrIn6, sin6_family), 1);
    assert_eq!(offset_of!(SockaddrIn6, sin6_scope_id), 24);

    assert_eq!(
        (size_of::<SockaddrUn>(), align_of::<SockaddrUn>()),
        (106, 1)
    );
    assert_eq!(offset_of!(SockaddrUn, sun_len), 0);
    assert_eq!(offset_of!(SockaddrUn, sun_family), 1);
    assert_eq!(offset_of!(SockaddrUn, sun_path), 2);

    assert_eq!(
        (size_of::<SockaddrStorage>(), align_of::<SockaddrStorage>()),
        (128, 8)
    );
    assert_eq!(offset_of!(SockaddrStorage, ss_len), 0);
    assert_eq!(offset_of!(SockaddrStorage, ss_family), 1);

    assert_eq!((size_of::<Msghdr>(), align_of::<Msghdr>()), (48, 8));
    assert_eq!(offset_of!(Msghdr, msg_flags), 44);

    assert_eq!((size_of::<Cmsghdr>(), align_of::<Cmsghdr>()), (12, 4));
    assert_eq!(offset_of!(Cmsghdr, cmsg_level), 4);
    assert_eq!(offset_of!(Cmsghdr, cmsg_type), 8);

    assert_eq!((size_of::<Addrinfo>(), align_of::<Addrinfo>()), (48, 8));
    assert_eq!(offset_of!(Addrinfo, ai_canonname), 24);
    assert_eq!(offset_of!(Addrinfo, ai_addr), 32);
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
