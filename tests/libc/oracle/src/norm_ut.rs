// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn with_errno_returns_the_value_and_a_cleared_errno() {
    crate::call::errno_set(17);
    let (value, errno) = with_errno(|| 7);
    assert_eq!(value, 7);
    assert_eq!(errno, 0);
}

#[test]
fn errno_set_and_get_round_trip() {
    crate::call::errno_set(42);
    assert_eq!(crate::call::errno_get(), 42);
    crate::call::errno_set(0);
}
