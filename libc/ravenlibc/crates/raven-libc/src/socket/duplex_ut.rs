// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ShellUnix` flag tests.

use super::*;
use raven_libc_abi::socket::SOCK_STREAM;

#[test]
fn stream_pair_has_no_extra_flags() {
    assert_eq!(duplex_flags(SOCK_STREAM, false, false), 0);
}

#[test]
fn datagram_pair_sets_the_message_mode_bit() {
    assert_eq!(duplex_flags(SOCK_DGRAM, false, false), DUPLEX_DATAGRAM);
}

#[test]
fn descriptor_flags_are_forwarded_to_the_kernel_pair() {
    assert_eq!(duplex_flags(SOCK_STREAM, true, false), DUPLEX_O_NONBLOCK);
    assert_eq!(duplex_flags(SOCK_STREAM, false, true), DUPLEX_O_CLOEXEC);
    assert_eq!(
        duplex_flags(SOCK_DGRAM, true, true),
        DUPLEX_DATAGRAM | DUPLEX_O_NONBLOCK | DUPLEX_O_CLOEXEC
    );
}
