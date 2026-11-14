// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `lsearch(3)`.
use core::ffi::c_void;

use raven_libc_stdlib::search::LSearchError;

use super::sort_support::{Compare, compare, elements, elements_ref};

/// Find `key`, appending it when absent.
///
/// # Safety
/// `base` has room for `(*nel + 1) * width` bytes; `key` has `width` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lsearch(
    key: *const c_void,
    base: *mut c_void,
    nel: *mut usize,
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
    // SAFETY: the caller's array has room for the append.
    let Some(elements) = (unsafe { elements(base, count.saturating_add(1), width) }) else {
        return crate::ffi::ret::null(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
    };
    let mut length = count;
    let result =
        raven_libc_stdlib::search::lsearch(key_bytes, elements, &mut length, width, &mut |a, b| {
            compare(func, a, b)
        });
    // SAFETY: the caller's count slot.
    unsafe { *nel = length };
    match result {
        // SAFETY: the index is inside the caller's array.
        Ok(index) => unsafe { base.cast::<u8>().add(index.wrapping_mul(width)).cast() },
        Err(LSearchError::NoSpace) => crate::ffi::ret::null(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOMEM as u16,
        )),
        Err(_) => crate::ffi::ret::null(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )),
    }
}
