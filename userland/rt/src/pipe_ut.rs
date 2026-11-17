// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pipe wrappers.

use super::{O_CLOEXEC, O_NONBLOCK, decode_fds};
use raven_abi::errno::Errno;
use raven_abi::types::Handle;

#[test]
fn decode_pair_preserves_both_handles() {
    let (read, write) = decode_fds([7, 9]).expect("valid descriptors");
    assert_eq!(read, Handle(7));
    assert_eq!(write, Handle(9));
}

#[test]
fn negative_descriptor_is_rejected() {
    assert_eq!(decode_fds([-1, 9]), Err(Errno::EBADF));
    assert_eq!(decode_fds([7, -1]), Err(Errno::EBADF));
}

#[test]
fn flags_match_the_documented_libc_values() {
    assert_eq!(O_NONBLOCK, 2048);
    assert_eq!(O_CLOEXEC, 524_288);
}
