// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn array_size_accepts_the_exact_boundary() {
    assert_eq!(array_size(2, 3), Some(6));
    assert_eq!(array_size(usize::MAX / 2, 2), Some(usize::MAX - 1));
}

#[test]
fn array_size_rejects_overflow() {
    assert_eq!(array_size(usize::MAX / 2 + 1, 2), None);
    assert_eq!(array_size(usize::MAX, 2), None);
}
