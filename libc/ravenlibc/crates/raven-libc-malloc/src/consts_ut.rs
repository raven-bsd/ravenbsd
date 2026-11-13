// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn page_size_is_the_only_4096() {
    assert_eq!(PAGE_SHIFT, 12);
    assert_eq!(PAGE_SIZE, 4096);
    assert_eq!(MALLOC_ALIGN, 16);
    assert_eq!(MAX_ALLOC, isize::MAX as usize);
}

#[test]
fn align_up_and_down_are_inverse_for_aligned_values() {
    for value in [0usize, 1, 15, 16, 17, 4095, 4096, 4097] {
        let up = align_up(value, 16).expect("align");
        assert_eq!(up % 16, 0);
        assert!(up >= value);
        assert_eq!(align_down(up, 16), Some(up));
    }
}

#[test]
fn align_rejects_zero_and_non_powers_of_two() {
    assert_eq!(align_up(1, 0), None);
    assert_eq!(align_up(1, 3), None);
    assert_eq!(align_down(1, 0), None);
    assert!(!is_aligned(3, 2));
    assert!(is_aligned(16, 16));
}

#[test]
fn align_up_reports_overflow() {
    assert_eq!(align_up(usize::MAX, 4096), None);
}

#[test]
fn page_constants_agree() {
    assert_eq!(PAGE_SIZE, 1usize << PAGE_SHIFT);
    assert!(PAGE_SIZE.is_power_of_two());
}

#[test]
fn max_alloc_is_isize_max() {
    assert_eq!(MAX_ALLOC, usize::MAX / 2);
}

#[test]
fn align_up_at_page_boundary_is_idempotent() {
    assert_eq!(align_up(4096, 4096), Some(4096));
    assert_eq!(align_up(4097, 4096), Some(8192));
}

#[test]
fn align_down_at_page_boundary() {
    assert_eq!(align_down(4096, 4096), Some(4096));
    assert_eq!(align_down(4095, 4096), Some(0));
}

#[test]
fn zero_is_aligned_to_everything() {
    assert!(is_aligned(0, 1));
    assert!(is_aligned(0, 16));
    assert!(is_aligned(0, 4096));
}
