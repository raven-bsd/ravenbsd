// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for wrapping sequence arithmetic.

use super::{seq_distance, seq_ge, seq_gt, seq_le, seq_lt};

#[test]
fn comparisons_across_the_wrap_point() {
    assert!(seq_lt(0xffff_ff00, 0x0000_0100));
    assert!(seq_gt(0x0000_0100, 0xffff_ff00));
    assert!(seq_le(0xffff_ffff, 0x0000_0000));
    assert!(seq_ge(0x0000_0000, 0xffff_ffff));
    assert!(!seq_lt(0x0000_0100, 0xffff_ff00));
}

#[test]
fn distance_is_wrapping() {
    assert_eq!(seq_distance(0xffff_fff0, 0x0000_0010), 0x20);
    assert_eq!(seq_distance(10, 10), 0);
}

#[test]
fn just_below_half_range_is_ordered() {
    assert!(seq_lt(0, 0x7fff_ffff));
    assert!(!seq_gt(0, 0x7fff_ffff));
}
