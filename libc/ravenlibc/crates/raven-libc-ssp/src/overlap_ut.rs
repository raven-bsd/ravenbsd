// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn identical_ranges_overlap() {
    assert!(ssp_overlap(0x1000, 0x1000, 16));
}

#[test]
fn adjacent_ranges_do_not_overlap() {
    assert!(!ssp_overlap(0x1000, 0x1010, 16));
    assert!(!ssp_overlap(0x1010, 0x1000, 16));
}

#[test]
fn containing_ranges_overlap() {
    assert!(ssp_overlap(0x1000, 0x1008, 32));
    assert!(ssp_overlap(0x1008, 0x1000, 32));
}

#[test]
fn zero_length_never_overlaps() {
    assert!(!ssp_overlap(0x1000, 0x1000, 0));
    assert!(!ssp_overlap(0, usize::MAX, 0));
}

#[test]
fn left_greater_than_right_is_symmetric() {
    let left = 0x2000usize;
    let right = 0x1000usize;
    assert_eq!(ssp_overlap(left, right, 16), ssp_overlap(right, left, 16));
}

#[test]
fn near_size_max_guards_against_overflow() {
    assert!(ssp_overlap(usize::MAX - 4, usize::MAX - 2, 8));
    assert!(ssp_overlap(usize::MAX - 2, usize::MAX - 4, 8));
    assert!(!ssp_overlap(0, 8, 8));
}

#[test]
fn page_boundaries() {
    assert!(!ssp_overlap(0x1000, 0x2000, 0x1000));
    assert!(ssp_overlap(0x1000, 0x1001, 2));
}

#[test]
fn one_byte_ranges() {
    assert!(ssp_overlap(5, 5, 1));
    assert!(!ssp_overlap(5, 6, 1));
}
