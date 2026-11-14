// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `lfind(3)`.
use core::ffi::c_void;

use super::sort_support::{Compare, compare, elements_ref};

/// Find `key` in a linear array; no insertion.
///
/// # Safety
/// `base` has `*nel * width` readable bytes; `key` has `width` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lfind(
    key: *const c_void,
    base: *const c_void,
    nel: *const usize,
    width: usize,
    compar: Option<Compare>,
) -> *mut c_void {
    // SAFETY: the caller's count slot is readable.
    let (Some(func), Some(count)) = (compar, unsafe { nel.as_ref().copied() }) else {
        return crate::ffi::ret::null(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
    };
    // SAFETY: the caller's key and array contracts.
    let Some(key_bytes) = (unsafe { elements_ref(key, 1, width) }) else {
        return core::ptr::null_mut();
    };
    // SAFETY: the caller's array has `count * width` readable bytes.
    let Some(elements) = (unsafe { elements_ref(base, count, width) }) else {
        return core::ptr::null_mut();
    };
    match raven_libc_stdlib::search::lfind(key_bytes, elements, count, width, &mut |a, b| {
        compare(func, a, b)
    }) {
        // SAFETY: the index is inside the caller's array.
        Some(index) => unsafe {
            base.cast_mut()
                .cast::<u8>()
                .add(index.wrapping_mul(width))
                .cast()
        },
        None => core::ptr::null_mut(),
    }
}
