// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `USER_COPY` argument validation.
//!
//! These exercise the parts that do not need a live address space: the call rejects a bad
//! range, an oversized length and an overlapping range before it copies. The successful copy
//! and the concurrent unmap path run in the target qtest, which has a real MMU.

use super::*;

#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::USER_END;
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::USER_END;

fn frame(destination: u64, source: u64, length: u64) -> SyscallFrame {
    SyscallFrame {
        rdi: destination,
        rsi: source,
        rdx: length,
        ..SyscallFrame::default()
    }
}

const BASE: u64 = 0x1000_0000;

#[test]
fn zero_length_succeeds_without_touching_the_pointers() {
    assert_eq!(user_copy(&frame(0, 0, 0)), Ok(0));
    assert_eq!(user_copy(&frame(u64::MAX, u64::MAX, 0)), Ok(0));
}

#[test]
fn kernel_half_and_all_ones_are_efault() {
    assert_eq!(
        user_copy(&frame(BASE, 0xFFFF_8000_0000_0000, 8)),
        Err(Errno::EFAULT)
    );
    assert_eq!(
        user_copy(&frame(0xFFFF_8000_0000_0000, BASE, 8)),
        Err(Errno::EFAULT)
    );
    assert_eq!(user_copy(&frame(BASE, u64::MAX, 8)), Err(Errno::EFAULT));
    assert_eq!(user_copy(&frame(u64::MAX, BASE, 8)), Err(Errno::EFAULT));
}

#[test]
fn null_and_out_of_window_are_efault() {
    assert_eq!(user_copy(&frame(BASE, 0, 8)), Err(Errno::EFAULT));
    assert_eq!(user_copy(&frame(0, BASE, 8)), Err(Errno::EFAULT));
    assert_eq!(user_copy(&frame(BASE, USER_END, 8)), Err(Errno::EFAULT));
    assert_eq!(user_copy(&frame(USER_END, BASE, 8)), Err(Errno::EFAULT));
}

#[test]
fn overlap_is_einval() {
    assert_eq!(user_copy(&frame(BASE + 1, BASE, 8)), Err(Errno::EINVAL));
    assert_eq!(user_copy(&frame(BASE, BASE + 1, 8)), Err(Errno::EINVAL));
    assert_eq!(user_copy(&frame(BASE, BASE, 8)), Err(Errno::EINVAL));
}

#[test]
fn one_byte_over_the_limit_is_einval() {
    assert_eq!(
        user_copy(&frame(BASE + 0x10_001, BASE, 0x10_001)),
        Err(Errno::EINVAL)
    );
}
