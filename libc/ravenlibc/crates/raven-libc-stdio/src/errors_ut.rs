// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::errors`].
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::bool_assert_comparison,
    clippy::undocumented_unsafe_blocks,
    clippy::uninlined_format_args
)]

use super::*;
use raven_libc_abi::generated::errno::{EBADF, EINVAL};

#[test]
fn fail_eof_has_no_errno() {
    assert_eq!(Fail::Eof.errno(), None);
}

#[test]
fn fail_errno_carries_the_value() {
    let fail = Fail::from_int(EBADF);
    assert_eq!(
        fail.errno(),
        Some(raven_libc_abi::errno::Errno(EBADF as u16))
    );
    assert_ne!(
        fail.errno(),
        Some(raven_libc_abi::errno::Errno(EINVAL as u16))
    );
    assert_eq!(
        Fail::from(raven_libc_abi::errno::Errno(7))
            .errno()
            .map(|e| e.0),
        Some(7)
    );
}

#[test]
fn eof_is_minus_one() {
    assert_eq!(EOF, -1);
}
