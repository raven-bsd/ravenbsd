// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `bsearch(3)`.
use core::ffi::c_void;

use super::sort_support::{Compare, elements_ref};

/// Find `key` in a sorted array.
///
/// # Safety
/// `base`/`key` are readable per the C contract; `compar` is a valid C comparator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bsearch(
    key: *const c_void,
    base: *const c_void,
    nmemb: usize,
    size: usize,
    compar: Option<Compare>,
) -> *mut c_void {
    let Some(func) = compar else {
        return core::ptr::null_mut();
    };
    // SAFETY: the caller's array contract.
    let Some(elements) = (unsafe { elements_ref(base, nmemb, size) }) else {
        return core::ptr::null_mut();
    };
    let found = raven_libc_stdlib::bsearch(elements, size, |element| {
        func(key, element.as_ptr().cast::<c_void>())
    });
    match found {
        // SAFETY: the index came from the caller's array.
        Some(index) => unsafe {
            base.cast_mut()
                .cast::<u8>()
                .add(index.wrapping_mul(size))
                .cast()
        },
        None => core::ptr::null_mut(),
    }
}
