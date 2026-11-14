// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn entry(base: *mut core::ffi::c_void, len: u64) -> Iovec {
    Iovec {
        iov_base: base,
        iov_len: len,
    }
}

#[test]
fn total_sums_the_lengths() {
    let entries = [
        entry(core::ptr::null_mut(), 3),
        entry(core::ptr::null_mut(), 5),
    ];
    assert_eq!(total(&entries), Ok(8));
}

#[test]
fn total_rejects_isize_max() {
    let entries = [entry(core::ptr::null_mut(), u64::MAX)];
    assert_eq!(total(&entries), Err(invalid()));
}

#[test]
fn negative_count_is_invalid() {
    // SAFETY: a negative count is rejected before any read.
    let error = unsafe { slices(core::ptr::null(), -1) }.expect_err("must reject");
    assert_eq!(i32::from(error.0), raven_libc_abi::errno::EINVAL);
}

#[test]
fn null_base_with_length_is_invalid() {
    let entries = [entry(core::ptr::null_mut(), 1)];
    // SAFETY: the entry slice is valid; the NULL base is the rejected condition.
    let error = unsafe { slices(entries.as_ptr(), 1) }.expect_err("must reject");
    assert_eq!(i32::from(error.0), raven_libc_abi::errno::EINVAL);
}

#[test]
fn readable_and_writable_views() {
    let data = [1u8, 2, 3, 4];
    let entries = [
        entry(data.as_ptr().cast_mut().cast(), 2),
        entry(data.as_ptr().cast_mut().cast(), 2),
    ];
    // SAFETY: two valid readable entries.
    let views = unsafe { slices(entries.as_ptr(), 2) }.expect("valid");
    assert_eq!(views, alloc::vec![&data[..2], &data[..2]]);
    let mut out = [0u8; 4];
    // SAFETY: the four-byte array has a valid pointer two bytes in.
    let second = unsafe { out.as_mut_ptr().add(2) };
    let writable = [entry(out.as_mut_ptr().cast(), 2), entry(second.cast(), 2)];
    // SAFETY: two valid disjoint writable entries.
    let mut views = unsafe { slices_mut(writable.as_ptr(), 2) }.expect("valid");
    views[0].copy_from_slice(&[7, 8]);
    views[1].copy_from_slice(&[9, 10]);
    assert_eq!(out, [7, 8, 9, 10]);
}
