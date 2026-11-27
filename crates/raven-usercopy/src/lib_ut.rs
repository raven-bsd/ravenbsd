// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pure user range and `USER_COPY` validators.

use super::*;

const START: u64 = 0x1000;
const END: u64 = 0x4000;

/// Base of a real in-window region for the `USER_COPY` cases.
const COPY_BASE: u64 = 0x1000_0000;
/// Length of one `MAX_USER_COPY` region.
const COPY_MAX: u64 = MAX_USER_COPY;

#[test]
fn normal_range() {
    assert!(UserSlice::new_in(0x2000, 0x100, START, END).is_ok());
}

#[test]
fn null_is_rejected() {
    assert_eq!(UserSlice::new_in(0, 1, START, END), Err(Errno::EFAULT));
}

#[test]
fn below_start_is_rejected() {
    assert_eq!(
        UserSlice::new_in(START - 1, 1, START, END),
        Err(Errno::EFAULT)
    );
}

#[test]
fn kernel_half_is_rejected() {
    assert_eq!(
        UserSlice::new_in(0xFFFF_8000_0000_0000, 1, START, END),
        Err(Errno::EFAULT)
    );
}

#[test]
fn end_overflow_is_rejected() {
    assert_eq!(
        UserSlice::new_in(END - 1, 2, START, END),
        Err(Errno::EFAULT)
    );
}

#[test]
fn exact_end_with_zero_length_is_allowed() {
    assert!(UserSlice::new_in(END, 0, START, END).is_ok());
}

#[test]
fn exact_end_with_data_is_rejected() {
    assert_eq!(UserSlice::new_in(END, 1, START, END), Err(Errno::EFAULT));
}

#[test]
fn zero_length_inside_range_is_allowed() {
    assert!(UserSlice::new_in(START, 0, START, END).is_ok());
}

#[test]
fn zero_length_below_range_is_rejected() {
    assert_eq!(
        UserSlice::new_in(START - 1, 0, START, END),
        Err(Errno::EFAULT)
    );
}

#[test]
fn length_above_isize_max_is_rejected() {
    assert_eq!(
        UserSlice::new_in(START, (isize::MAX as u64) + 1, START, END),
        Err(Errno::EFAULT)
    );
}

#[test]
fn max_length_is_rejected_without_overflow() {
    assert_eq!(
        UserSlice::new_in(START, u64::MAX, START, END),
        Err(Errno::EFAULT)
    );
}

#[test]
fn exact_boundary_is_accepted() {
    assert!(UserSlice::new_in(START, END - START, START, END).is_ok());
}

#[test]
fn typed_pointer_checks_size() {
    let pointer = UserPtr::<u64>::new_in(START, START, END).expect("aligned pointer");
    assert_eq!(pointer.address(), START);
}

#[test]
fn typed_pointer_rejects_misalignment() {
    assert_eq!(
        UserPtr::<u32>::new_in(START + 1, START, END),
        Err(Errno::EFAULT)
    );
}

#[test]
fn typed_pointer_needs_full_object() {
    assert_eq!(
        UserPtr::<u64>::new_in(END - 4, START, END),
        Err(Errno::EFAULT)
    );
}

#[test]
fn user_address_helper() {
    assert!(is_user_address(USER_MIN));
    assert!(is_user_address(USER_END - 1));
    assert!(!is_user_address(0));
    assert!(!is_user_address(USER_END));
    assert!(!is_user_address(0xFFFF_8000_0000_0000));
}

#[test]
fn invalid_range_is_rejected() {
    assert_eq!(UserSlice::new_in(0x2000, 1, END, START), Err(Errno::EFAULT));
}

#[test]
fn user_copy_accepts_disjoint_and_adjacent_ranges() {
    assert!(validate_user_copy(COPY_BASE + 0x1000, COPY_BASE, 8).is_ok());
    // Adjacent both directions are not overlap.
    assert!(validate_user_copy(COPY_BASE + 8, COPY_BASE, 8).is_ok());
    assert!(validate_user_copy(COPY_BASE, COPY_BASE + 8, 8).is_ok());
}

#[test]
fn user_copy_rejects_forward_backward_and_identical_overlap() {
    // forward: dst starts one byte into the source range.
    assert_eq!(
        validate_user_copy(COPY_BASE + 1, COPY_BASE, 8),
        Err(Errno::EINVAL)
    );
    // backward: src starts one byte into the destination range.
    assert_eq!(
        validate_user_copy(COPY_BASE, COPY_BASE + 1, 8),
        Err(Errno::EINVAL)
    );
    // identical.
    assert_eq!(
        validate_user_copy(COPY_BASE, COPY_BASE, 8),
        Err(Errno::EINVAL)
    );
}

#[test]
fn user_copy_zero_length_needs_no_pointer() {
    assert!(validate_user_copy(0, 0, 0).is_ok());
    assert!(validate_user_copy(u64::MAX, u64::MAX, 0).is_ok());
    assert!(validate_user_copy(0, COPY_BASE, 0).is_ok());
}

#[test]
fn user_copy_accepts_the_maximum_length() {
    let destination = COPY_BASE + COPY_MAX;
    assert!(validate_user_copy(destination, COPY_BASE, COPY_MAX).is_ok());
}

#[test]
fn user_copy_rejects_one_byte_over_the_maximum() {
    let length = COPY_MAX + 1;
    let destination = COPY_BASE + COPY_MAX + 1;
    assert_eq!(
        validate_user_copy(destination, COPY_BASE, length),
        Err(Errno::EINVAL)
    );
}

#[test]
fn user_copy_rejects_kernel_half_source_and_destination() {
    assert_eq!(
        validate_user_copy(COPY_BASE, 0xFFFF_8000_0000_0000, 8),
        Err(Errno::EFAULT)
    );
    assert_eq!(
        validate_user_copy(0xFFFF_8000_0000_0000, COPY_BASE, 8),
        Err(Errno::EFAULT)
    );
}

#[test]
fn user_copy_rejects_out_of_window_and_overflowing_ranges() {
    // A NULL source/destination with a positive length.
    assert_eq!(validate_user_copy(COPY_BASE, 0, 8), Err(Errno::EFAULT));
    assert_eq!(validate_user_copy(0, COPY_BASE, 8), Err(Errno::EFAULT));
    // The range crosses the end of the window.
    assert_eq!(
        validate_user_copy(COPY_BASE, USER_END - 4, 8),
        Err(Errno::EFAULT)
    );
    // Out-of-window source and destination.
    assert_eq!(
        validate_user_copy(COPY_BASE, USER_END, 8),
        Err(Errno::EFAULT)
    );
    assert_eq!(
        validate_user_copy(USER_END, COPY_BASE, 8),
        Err(Errno::EFAULT)
    );
}

#[test]
fn user_copy_rejects_the_all_ones_pointer() {
    assert_eq!(
        validate_user_copy(COPY_BASE, u64::MAX, 8),
        Err(Errno::EFAULT)
    );
    assert_eq!(
        validate_user_copy(u64::MAX, COPY_BASE, 8),
        Err(Errno::EFAULT)
    );
}
