// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Tests for the integer-trait `ilog2`, which must not reference core's hidden
//! `int_log10::panic_for_nonpositive_argument` symbol (C65m link closure).

use super::Int;

#[test]
fn ilog2_matches_core_for_nonzero_values() {
    for value in 1u32..=4096 {
        assert_eq!(<u32 as Int>::ilog2(value), value.ilog2(), "u32 {value}");
    }
    for value in [u32::MAX, 1 << 31, (1 << 31) + 1] {
        assert_eq!(<u32 as Int>::ilog2(value), value.ilog2(), "u32 {value}");
    }
    for value in [1u64, 2, 3, 1 << 63, u64::MAX] {
        assert_eq!(<u64 as Int>::ilog2(value), value.ilog2(), "u64 {value}");
    }
}

#[test]
fn ilog2_is_total_at_zero() {
    assert_eq!(<u32 as Int>::ilog2(0), 0);
    assert_eq!(<u64 as Int>::ilog2(0), 0);
    assert_eq!(<u16 as Int>::ilog2(0), 0);
}
