// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `strto*` status table.

use super::*;
use crate::ffi::errno;

#[test]
fn ok_leaves_errno_alone() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(5);
    apply(IntStatus::Ok);
    assert_eq!(errno::get(), 5);
}

#[test]
fn no_digits_leaves_errno_alone() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(5);
    apply(IntStatus::NoDigits);
    assert_eq!(errno::get(), 5);
}

#[test]
fn range_is_erange() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(0);
    apply(IntStatus::Range);
    assert_eq!(errno::get(), 34);
}

#[test]
fn invalid_base_is_einval() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(0);
    apply(IntStatus::InvalidBase);
    assert_eq!(errno::get(), 22);
}
