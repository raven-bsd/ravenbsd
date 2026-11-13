// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Iovec and path tests.

use super::*;
use crate::ErrnoExt;
use raven_libc_abi::socket::{SOCK_DGRAM, SOCK_STREAM};

#[test]
fn gather_three_parts() {
    let mut out = [0u8; 12];
    let used = gather(&[b"abc", b"de", b"f"], &mut out).expect("gather");
    assert_eq!(used, 6);
    assert_eq!(&out[..6], b"abcdef");
}

#[test]
fn scatter_three_parts() {
    let data = *b"abcdefghij";
    let mut first = [0u8; 4];
    let mut second = [0u8; 4];
    let mut third = [0u8; 4];
    let used = scatter(
        &data,
        &mut [&mut first[..], &mut second[..], &mut third[..]],
    );
    assert_eq!(used, 10);
    assert_eq!(&first, b"abcd");
    assert_eq!(&second, b"efgh");
    assert_eq!(&third[..2], b"ij");
}

#[test]
fn gather_rejects_overlong_datagram() {
    let mut out = [0u8; 4];
    assert_eq!(gather(&[&[1u8; 5]], &mut out), Err(Errno::EMSGSIZE));
}

#[test]
fn total_length_is_bounded_arithmetic() {
    assert_eq!(total_len(&[b"a", b"bc"]), 3);
}

#[test]
fn tcp_path_vector() {
    let mut buffer = [0u8; 64];
    let path = socket_path(
        SOCK_STREAM,
        Endpoint {
            ip: [10, 0, 0, 2],
            port: 80,
        },
        &mut buffer,
    )
    .expect("path");
    assert_eq!(path, "net:tcp/10.0.0.2:80");
}

#[test]
fn udp_path_vector() {
    let mut buffer = [0u8; 64];
    let path = socket_path(
        SOCK_DGRAM,
        Endpoint {
            ip: [192, 0, 2, 7],
            port: 53,
        },
        &mut buffer,
    )
    .expect("path");
    assert_eq!(path, "net:udp/192.0.2.7:53");
}

#[test]
fn port_zero_is_refused() {
    let mut buffer = [0u8; 64];
    assert_eq!(
        socket_path(
            SOCK_STREAM,
            Endpoint {
                ip: [10, 0, 0, 2],
                port: 0
            },
            &mut buffer
        ),
        Err(Errno::ECONNREFUSED)
    );
}

#[test]
fn bare_paths() {
    let mut buffer = [0u8; 16];
    assert_eq!(bare_path(SOCK_STREAM, &mut buffer), Ok("net:tcp"));
    assert_eq!(bare_path(SOCK_DGRAM, &mut buffer), Ok("net:udp"));
}
