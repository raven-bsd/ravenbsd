// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the result/errno mapping.

use super::*;
use crate::ffi::errno;

#[test]
fn int_passes_the_value_through() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(5);
    assert_eq!(int(Ok(12)), 12);
}

#[test]
fn int_leaves_errno_untouched_on_success() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(5);
    let _ = int(Ok(0));
    assert_eq!(errno::get(), 5);
}

#[test]
fn int_sets_errno_and_returns_minus_one() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(0);
    assert_eq!(int(Err(Errno(22))), -1);
    assert_eq!(errno::get(), 22);
}

#[test]
fn long_has_the_same_contract() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(0);
    assert_eq!(long(Ok(9)), 9);
    assert_eq!(long(Err(Errno(34))), -1);
    assert_eq!(errno::get(), 34);
}

#[test]
fn size_returns_the_c_sentinel_on_error() {
    let _guard = crate::ffi::TestLock::acquire();
    assert_eq!(size(Ok(3)), 3);
    assert_eq!(size(Err(Errno(12))), usize::MAX);
}

#[test]
fn ssize_returns_minus_one_on_error() {
    let _guard = crate::ffi::TestLock::acquire();
    assert_eq!(ssize(Ok(4)), 4);
    assert_eq!(ssize(Err(Errno(5))), -1);
}

#[test]
fn ptr_returns_null_and_sets_errno() {
    let _guard = crate::ffi::TestLock::acquire();
    let value = ptr::<u8>(Err(Errno(12)));
    assert!(value.is_null());
    assert_eq!(errno::get(), 12);
}

#[test]
fn null_sets_errno() {
    let _guard = crate::ffi::TestLock::acquire();
    let value = null::<u8>(Errno(2));
    assert!(value.is_null());
    assert_eq!(errno::get(), 2);
}

#[test]
fn unit_maps_ok_to_zero() {
    let _guard = crate::ffi::TestLock::acquire();
    assert_eq!(unit(Ok(())), 0);
    assert_eq!(unit(Err(Errno(22))), -1);
}

#[test]
fn platform_results_map_like_every_other() {
    let _guard = crate::ffi::TestLock::acquire();
    assert_eq!(platform_int(Ok(7)), 7);
    assert_eq!(platform_int(Err(raven_libc_platform::Errno::ENOENT)), -1);
    assert_eq!(errno::get(), 2);
}

#[test]
fn platform_ssize_maps_a_short_count() {
    let _guard = crate::ffi::TestLock::acquire();
    assert_eq!(platform_ssize(Ok(10)), 10);
    assert_eq!(platform_ssize(Err(raven_libc_platform::Errno::EBADF)), -1);
    assert_eq!(errno::get(), 9);
}

#[test]
fn an_oversized_platform_count_is_eoverflow() {
    let _guard = crate::ffi::TestLock::acquire();
    assert_eq!(platform_ssize(Ok(usize::MAX)), -1);
    assert_eq!(errno::get(), 75);
}
