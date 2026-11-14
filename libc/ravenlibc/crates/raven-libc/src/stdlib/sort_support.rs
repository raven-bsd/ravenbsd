// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Comparator and buffer adapters shared by the `<stdlib.h>`/`<search.h>` sort exports.

use core::ffi::{c_int, c_void};

use raven_libc_stdlib::SortError;

/// The C `qsort`/`bsearch` comparator.
pub(crate) type Compare = extern "C" fn(*const c_void, *const c_void) -> c_int;
/// The GNU `qsort_r` comparator with its context pointer.
pub(crate) type CompareR = extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int;

/// A writable element array, or `None` for a rejected size.
///
/// # Safety
/// `base` points to `nmemb * size` writable bytes.
pub(crate) unsafe fn elements<'a>(
    base: *mut c_void,
    nmemb: usize,
    size: usize,
) -> Option<&'a mut [u8]> {
    let total = nmemb.checked_mul(size)?;
    if total == 0 || base.is_null() {
        return None;
    }
    // SAFETY: the caller's array contract.
    Some(unsafe { core::slice::from_raw_parts_mut(base.cast::<u8>(), total) })
}

/// A readable element array, or `None` for a rejected size.
///
/// # Safety
/// `base` points to `nmemb * size` readable bytes.
pub(crate) unsafe fn elements_ref<'a>(
    base: *const c_void,
    nmemb: usize,
    size: usize,
) -> Option<&'a [u8]> {
    let total = nmemb.checked_mul(size)?;
    if total == 0 || base.is_null() {
        return None;
    }
    // SAFETY: the caller's array contract.
    Some(unsafe { core::slice::from_raw_parts(base.cast::<u8>(), total) })
}

/// Call the C comparator on two element slices.
pub(crate) fn compare(func: Compare, a: &[u8], b: &[u8]) -> c_int {
    func(a.as_ptr().cast::<c_void>(), b.as_ptr().cast::<c_void>())
}

/// Call the GNU comparator on two element slices and the context.
pub(crate) fn compare_r(func: CompareR, arg: *mut c_void, a: &[u8], b: &[u8]) -> c_int {
    func(
        a.as_ptr().cast::<c_void>(),
        b.as_ptr().cast::<c_void>(),
        arg,
    )
}

/// Map a C41 sort refusal to the C `-1`/`errno` form.
pub(crate) fn sort_result(result: Result<(), SortError>) -> c_int {
    match result {
        Ok(()) => 0,
        Err(SortError::Inval) => crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ))),
        Err(SortError::NoMem) => crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOMEM as u16,
        ))),
    }
}
