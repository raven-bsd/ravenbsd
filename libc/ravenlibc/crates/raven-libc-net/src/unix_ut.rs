// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `AF_UNIX` seam tests.

use super::*;
use crate::ErrnoExt;
use crate::FakeNetd;
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

#[test]
fn no_unix_pair_flags_still_reports_eafnosupport() {
    assert_eq!(
        NoUnix.pair_flags(SOCK_STREAM, true, true),
        Err(Errno::EAFNOSUPPORT)
    );
}

#[test]
fn shutdown_selectors_roundtrip_the_posix_values() {
    assert_eq!(UnixShutdown::from_posix(0), Some(UnixShutdown::Read));
    assert_eq!(UnixShutdown::from_posix(1), Some(UnixShutdown::Write));
    assert_eq!(UnixShutdown::from_posix(2), Some(UnixShutdown::Both));
    assert_eq!(UnixShutdown::from_posix(3), None);
    assert_eq!(UnixShutdown::from_posix(-1), None);
    assert_eq!(UnixShutdown::Read.as_posix(), 0);
    assert_eq!(UnixShutdown::Write.as_posix(), 1);
    assert_eq!(UnixShutdown::Both.as_posix(), 2);
}

struct PairOnly;

impl UnixBackend for PairOnly {
    fn pair(&self, ty: i32) -> Result<(Sock, Sock), Errno> {
        Ok((Sock(ty as u32), Sock(1)))
    }

    fn bind(&self, _path: &[u8]) -> Result<Sock, Errno> {
        Err(Errno::EAFNOSUPPORT)
    }

    fn connect(&self, _path: &[u8]) -> Result<Sock, Errno> {
        Err(Errno::EAFNOSUPPORT)
    }
}

#[test]
fn default_pair_flags_delegates_to_pair() {
    assert_eq!(
        PairOnly.pair_flags(SOCK_STREAM, true, true),
        Ok((Sock(SOCK_STREAM as u32), Sock(1)))
    );
}

#[test]
fn default_data_operations_use_the_net_backend() {
    let fake = FakeNetd::new();
    let (left, right) = fake.pair(SOCK_STREAM).expect("pair");
    assert_eq!(UnixBackend::unix_write(&fake, &fake, left, b"ping"), Ok(4));
    let mut buffer = [0u8; 8];
    let done = UnixBackend::unix_read(&fake, &fake, right, &mut buffer).expect("read");
    assert_eq!(done.len, 4);
    assert_eq!(&buffer[..4], b"ping");
}
