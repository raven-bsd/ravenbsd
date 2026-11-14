// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `qsort_r(3)` (GNU argument order).
use core::ffi::c_void;

use super::sort_support::{CompareR, compare_r, elements};

/// Sort with a context pointer passed to the comparator.
///
/// # Safety
/// `base` points to `nmemb * size` writable bytes; `compar` is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qsort_r(
    base: *mut c_void,
    nmemb: usize,
    size: usize,
    compar: Option<CompareR>,
    arg: *mut c_void,
) {
    let Some(func) = compar else {
        return;
    };
    // SAFETY: the caller's array contract.
    let Some(elements) = (unsafe { elements(base, nmemb, size) }) else {
        return;
    };
    raven_libc_stdlib::qsort(elements, size, |a, b| compare_r(func, arg, a, b));
}
