// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Option storage tests.

use super::*;
use crate::ErrnoExt;
use raven_libc_abi::netinet::TCP_NODELAY;
use raven_libc_abi::socket::{SO_ERROR, SO_KEEPALIVE, SO_RCVBUF, SO_REUSEADDR};

#[test]
fn wire_ids_match_the_donor_table() {
    assert_eq!(
        wire_option(SOL_SOCKET, SO_REUSEADDR),
        Some(SockOpt::ReuseAddr)
    );
    assert_eq!(wire_option(SOL_SOCKET, SO_RCVBUF), Some(SockOpt::RcvBuf));
    assert_eq!(wire_option(SOL_SOCKET, SO_SNDBUF), Some(SockOpt::SndBuf));
    assert_eq!(
        wire_option(IPPROTO_TCP, TCP_NODELAY),
        Some(SockOpt::TcpNoDelay)
    );
    assert_eq!(wire_option(SOL_SOCKET, SO_ERROR), Some(SockOpt::Error));
    assert_eq!(wire_option(SOL_SOCKET, 999), None);
}

#[test]
fn stored_names_are_recognised() {
    assert!(stored_option(SOL_SOCKET, SO_KEEPALIVE));
    assert!(stored_option(SOL_SOCKET, SO_REUSEPORT));
    assert!(stored_option(SOL_SOCKET, SO_LINGER));
    assert!(stored_option(SOL_SOCKET, SO_BROADCAST));
    assert!(stored_option(SOL_SOCKET, SO_OOBINLINE));
    assert!(stored_option(IPPROTO_IPV6, IPV6_V6ONLY));
    assert!(!stored_option(SOL_SOCKET, SO_REUSEADDR));
}

#[test]
fn i32_roundtrip() {
    let mut bytes = [0u8; 4];
    assert_eq!(write_i32(&mut bytes, -7), Ok(4));
    assert_eq!(read_i32(&bytes), Ok(-7));
    assert_eq!(read_i32(&[0u8; 3]), Err(Errno::EINVAL));
    assert_eq!(write_i32(&mut [0u8; 3], 1), Err(Errno::EINVAL));
}

#[test]
fn linger_roundtrip() {
    let value = [1, 0, 0, 0, 2, 0, 0, 0];
    assert_eq!(read_linger(&value), Ok([1, 2]));
    assert_eq!(read_linger(&value[..4]), Err(Errno::EINVAL));
}

#[test]
fn timeval_roundtrip() {
    let mut bytes = [0u8; 16];
    assert_eq!(write_timeval(&mut bytes, 1, 500), Ok(16));
    assert_eq!(read_timeval(&bytes), Ok((1, 500)));
    assert_eq!(read_timeval(&bytes[..8]), Err(Errno::EINVAL));
}

#[test]
fn timeout_milliseconds() {
    assert_eq!(timeout_ms(0, 0), None);
    assert_eq!(timeout_ms(1, 500_000), Some(1500));
    assert_eq!(timeout_ms(0, 50_000), Some(50));
}

#[test]
fn record_options_are_answered_locally() {
    assert_eq!(record_option(SO_TYPE), Some(RecordOption::Type));
    assert_eq!(record_option(SO_DOMAIN), Some(RecordOption::Domain));
    assert_eq!(record_option(SO_PROTOCOL), Some(RecordOption::Protocol));
    assert_eq!(record_option(SO_ACCEPTCONN), Some(RecordOption::AcceptConn));
    assert_eq!(record_option(SO_RCVBUF), None);
}
