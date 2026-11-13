// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Helpers shared by the pointer shell: slice construction and the word-at-a-time tests.
//!
//! Every helper is `unsafe` and its caller states the C-level contract in a `// SAFETY:` comment.

/// `HASZERO(x)` of musl: true when `x` has a zero byte.
#[inline(always)]
pub(crate) fn has_zero(x: usize) -> bool {
    const ONES: usize = usize::MAX / 255;
    const HIGHS: usize = ONES * 128;
    (x.wrapping_sub(ONES) & !x & HIGHS) != 0
}

/// The multiplier that turns a byte into `ONES * c` for the `HASZERO` word trick.
#[inline(always)]
pub(crate) fn ones_times(c: u8) -> usize {
    const ONES: usize = usize::MAX / 255;
    ONES * usize::from(c)
}

/// Build a read slice of exactly `n` bytes; a zero length yields `&[]`, never a null dereference.
///
/// # Safety
///
/// `p` is readable for `n` bytes (or `n == 0`, where `p` may be dangling).
#[inline(always)]
pub(crate) unsafe fn bytes<'a>(p: *const u8, n: usize) -> &'a [u8] {
    if n == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `p` is readable for `n` bytes; the lifetime is the
        // caller's, bounded by the C call contract.
        unsafe { core::slice::from_raw_parts(p, n) }
    }
}

/// Build a writable slice of exactly `n` bytes; a zero length yields `&mut []`.
///
/// # Safety
///
/// `p` is writable for `n` bytes (or `n == 0`, where `p` may be dangling).
#[inline(always)]
pub(crate) unsafe fn bytes_mut<'a>(p: *mut u8, n: usize) -> &'a mut [u8] {
    if n == 0 {
        &mut []
    } else {
        // SAFETY: the caller guarantees `p` is writable for `n` bytes and is not aliased.
        unsafe { core::slice::from_raw_parts_mut(p, n) }
    }
}
