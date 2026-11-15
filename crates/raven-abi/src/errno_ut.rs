// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the errno table.

use super::*;

#[test]
fn roundtrip() {
    assert_eq!(Errno::from_ret(Errno::ENOENT.to_ret()), Err(Errno::ENOENT));
    assert_eq!(Errno::from_ret(0), Ok(0));
    assert_eq!(Errno::from_ret(42), Ok(42));
}

#[test]
fn linux_values() {
    assert_eq!(Errno::EPERM.0, 1);
    assert_eq!(Errno::EAGAIN.0, 11);
    assert_eq!(Errno::ENOSYS.0, 38);
    assert_eq!(Errno::ETIMEDOUT.0, 110);
}

#[test]
fn pg29_values_are_the_linux_numbers() {
    assert_eq!(Errno::ENOTBLK.0, 15);
    assert_eq!(Errno::EDEADLK.0, 35);
    assert_eq!(Errno::ENOMSG.0, 42);
    assert_eq!(Errno::EBADMSG.0, 74);
    assert_eq!(Errno::EILSEQ.0, 84);
    assert_eq!(Errno::ENETDOWN.0, 100);
    assert_eq!(Errno::ESTALE.0, 116);
    assert_eq!(Errno::EOWNERDEAD.0, 130);
    assert_eq!(Errno::ENOTRECOVERABLE.0, 131);
}

#[test]
fn aliases_keep_the_canonical_values() {
    assert_eq!(Errno::EWOULDBLOCK, Errno::EAGAIN);
    assert_eq!(Errno::ENOTSUP, Errno::EOPNOTSUPP);
    assert_eq!(Errno::EDEADLOCK, Errno::EDEADLK);
}

#[test]
fn high_addresses_are_not_errors() {
    // A mapping returned at 0xFFFF_FFFF_FFFF_F000 is -4096 as isize: success.
    assert_eq!(Errno::from_ret(-4096), Ok((-4096isize) as usize));
    assert_eq!(Errno::from_ret(-4095), Err(Errno(4095)));
}

#[test]
fn descriptions() {
    assert_eq!(Errno::ENOENT.description(), "no such file or directory");
    assert_eq!(Errno::EILSEQ.description(), "illegal byte sequence");
    assert_eq!(Errno(4000).description(), "unknown error");
}
