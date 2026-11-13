// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `errno` cell.

use super::*;
use crate::ffi::TestLock;

#[test]
fn set_and_get_round_trip() {
    let _guard = TestLock::acquire();
    set_raw(7);
    assert_eq!(get(), 7);
    set(Errno(34));
    assert_eq!(get(), 34);
}

#[test]
fn the_pointer_reaches_the_same_cell() {
    let _guard = crate::ffi::TestLock::acquire();
    set_raw(0);
    // SAFETY: `ptr()` is the address of the static cell.
    unsafe { *ptr() = 99 };
    assert_eq!(get(), 99);
}

#[test]
fn platform_errors_store_their_number() {
    let _guard = crate::ffi::TestLock::acquire();
    set_platform(raven_libc_platform::Errno::ENOSYS);
    assert_eq!(get(), 38);
}

#[test]
fn a_zero_value_is_stored_like_any_other() {
    let _guard = crate::ffi::TestLock::acquire();
    set_raw(12);
    set_raw(0);
    assert_eq!(get(), 0);
}
