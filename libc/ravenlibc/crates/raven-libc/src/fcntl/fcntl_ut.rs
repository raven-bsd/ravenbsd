// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the record-lock pointer hand-off.

use super::*;

#[test]
fn a_null_lock_pointer_is_efault() {
    assert_eq!(record_lock(3, 6, 0), Err(Errno::EFAULT));
    assert_eq!(record_lock(3, 5, 0), Err(Errno::EFAULT));
}
