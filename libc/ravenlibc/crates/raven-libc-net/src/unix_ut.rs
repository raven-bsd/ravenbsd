// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `AF_UNIX` default tests.

use super::*;
use crate::ErrnoExt;
use raven_libc_abi::socket::{SOCK_DGRAM, SOCK_STREAM};

#[test]
fn no_unix_pair_is_eafnosupport() {
    assert_eq!(NoUnix.pair(SOCK_STREAM), Err(Errno::EAFNOSUPPORT));
    assert_eq!(NoUnix.pair(SOCK_DGRAM), Err(Errno::EAFNOSUPPORT));
}

#[test]
fn no_unix_bind_and_connect_are_eafnosupport() {
    assert_eq!(NoUnix.bind(b"/tmp/sock"), Err(Errno::EAFNOSUPPORT));
    assert_eq!(NoUnix.connect(b"/tmp/sock"), Err(Errno::EAFNOSUPPORT));
}
