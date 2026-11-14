// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mergesort(3)`.
use core::ffi::{c_int, c_void};

use super::sort_support::{Compare, compare, elements, sort_result};

/// Sort with the BSD stable merge sort.
///
/// # Safety
/// `base` points to `nmemb * size` writable bytes; `compar` is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mergesort(
    base: *mut c_void,
    nmemb: usize,
    size: usize,
    compar: Option<Compare>,
) -> c_int {
    let Some(func) = compar else {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    };
    // SAFETY: the caller's array contract.
    let Some(elements) = (unsafe { elements(base, nmemb, size) }) else {
        return 0;
    };
    sort_result(raven_libc_stdlib::mergesort(elements, size, |a, b| {
        compare(func, a, b)
    }))
}
