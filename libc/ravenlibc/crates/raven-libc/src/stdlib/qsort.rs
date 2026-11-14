// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `qsort(3)`.
use core::ffi::c_void;

use super::sort_support::{Compare, compare, elements};

/// Sort `nmemb` elements of `size` bytes.
///
/// # Safety
/// `base` points to `nmemb * size` writable bytes; `compar` is a valid C comparator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsort(
    base: *mut c_void,
    nmemb: usize,
    size: usize,
    compar: Option<Compare>,
) {
    let Some(func) = compar else {
        return;
    };
    // SAFETY: the caller's array contract.
    let Some(elements) = (unsafe { elements(base, nmemb, size) }) else {
        return;
    };
    raven_libc_stdlib::qsort(elements, size, |a, b| compare(func, a, b));
}
