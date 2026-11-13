// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `MSG_*` and socket-type tests.

use super::*;
use crate::ErrnoExt;
use raven_libc_abi::socket::{
    MSG_DONTWAIT, MSG_NOSIGNAL, MSG_OOB, MSG_PEEK, SOCK_RAW, SOCK_STREAM,
};

#[test]
fn known_flags_are_accepted() {
    assert_eq!(validate(0), Ok(()));
    assert_eq!(validate(MSG_DONTWAIT | MSG_NOSIGNAL), Ok(()));
    assert_eq!(validate(MSG_PEEK), Ok(()));
}

#[test]
fn unknown_flag_bits_are_einval() {
    assert_eq!(validate(0x8000_0000u32 as i32), Err(Errno::EINVAL));
}

#[test]
fn send_rejects_oob_more_confirm() {
    assert_eq!(send_supported(MSG_OOB), Err(Errno::EOPNOTSUPP));
    assert_eq!(send_supported(MSG_MORE), Err(Errno::EOPNOTSUPP));
    assert_eq!(send_supported(MSG_CONFIRM), Err(Errno::EOPNOTSUPP));
    assert_eq!(send_supported(MSG_DONTWAIT), Ok(()));
}

#[test]
fn recv_rejects_oob() {
    assert_eq!(recv_supported(MSG_OOB), Err(Errno::EOPNOTSUPP));
    assert_eq!(recv_supported(MSG_PEEK | MSG_WAITALL), Ok(()));
}

#[test]
fn socket_type_split() {
    assert_eq!(split_type(SOCK_STREAM), Ok((1, false, false)));
    assert_eq!(split_type(SOCK_STREAM | SOCK_CLOEXEC), Ok((1, true, false)));
    assert_eq!(split_type(SOCK_DGRAM | SOCK_NONBLOCK), Ok((2, false, true)));
    assert_eq!(split_type(SOCK_RAW), Err(Errno::ESOCKTNOSUPPORT));
    assert_eq!(split_type(1 << 10), Err(Errno::EINVAL));
}
