// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn multiplication_boundaries() {
    assert_eq!(mul_overflows(0, usize::MAX), Some(0));
    assert_eq!(mul_overflows(1, usize::MAX), Some(usize::MAX));
    assert_eq!(mul_overflows(usize::MAX, 2), None);
}

#[test]
fn array_threshold_is_sqrt_of_size_max_plus_one() {
    assert_eq!(array_threshold(), 1 << (usize::BITS / 2));
    assert_eq!(array_size(usize::MAX / 2 + 1, 2), None);
    assert_eq!(array_size(0, usize::MAX), Some(0));
    assert_eq!(array_size(1024, 1024), Some(1_048_576));
}

#[test]
fn alignment_rules() {
    assert!(align_ok(1));
    assert!(align_ok(2));
    assert!(align_ok(8));
    assert!(align_ok(4096));
    assert!(!align_ok(0));
    assert!(!align_ok(3));
    assert!(!align_ok(12));
}

#[test]
fn posix_alignment_is_a_multiple_of_pointer_size() {
    assert!(posix_align_ok(8));
    assert!(posix_align_ok(16));
    assert!(!posix_align_ok(4));
    assert!(!posix_align_ok(0));
}

#[test]
fn alignment_overflow_is_detected() {
    assert!(alignment_overflows(16, usize::MAX - 8));
    assert!(!alignment_overflows(16, 8));
}

#[test]
fn array_size_handles_zero_operands() {
    assert_eq!(array_size(0, 0), Some(0));
    assert_eq!(array_size(1, 0), Some(0));
    assert_eq!(array_size(0, 1), Some(0));
}

#[test]
fn array_size_accepts_one_by_max() {
    assert_eq!(array_size(1, usize::MAX), Some(usize::MAX));
}

#[test]
fn posix_alignment_rejects_one() {
    assert!(!posix_align_ok(1));
    assert!(!posix_align_ok(2));
}

#[test]
fn overflow_check_accepts_exact_fit() {
    assert!(!alignment_overflows(0, usize::MAX));
    assert!(!alignment_overflows(1, usize::MAX - 1));
}

#[test]
fn multiplication_accepts_square_root_boundary() {
    assert_eq!(mul_overflows(2, usize::MAX / 2), Some(usize::MAX - 1));
}
