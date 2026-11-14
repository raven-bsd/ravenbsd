// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<wchar.h>` string exports backed by `raven-libc-locale::wstring`.
//!
//! The shell converts the C `wchar_t *` pair to slices; no algorithm lives here. `wcstok` and the
//! wide-FILE family stay in `expected/DEFERRED.txt` until their shell state (C60c/C64).

use core::ffi::{c_int, c_void};

use raven_libc_locale::wstring;

unsafe fn len(pointer: *const u32) -> usize {
    if pointer.is_null() {
        return 0;
    }
    let mut length = 0usize;
    loop {
        // SAFETY: the C string contract guarantees a readable unit at every step up to NUL.
        if unsafe { *pointer.add(length) } == 0 {
            return length;
        }
        length = length.saturating_add(1);
    }
}

unsafe fn slice<'a>(pointer: *const u32) -> &'a [u32] {
    // SAFETY: the caller's NUL-terminated string contract; `len` only reads up to the NUL.
    let length = unsafe { len(pointer) };
    if pointer.is_null() {
        return &[];
    }
    // SAFETY: `len` found a NUL within the caller's string.
    unsafe { core::slice::from_raw_parts(pointer, length) }
}

/// `wcslen`.
///
/// # Safety
/// `s` is NUL terminated.
pub(crate) unsafe extern "C" fn wcslen_impl(s: *const u32) -> usize {
    // SAFETY: `s` is NUL terminated by the C contract.
    unsafe { len(s) }
}

/// `wcscpy`.
///
/// # Safety
/// `dst` is writable for the source length plus NUL; `src` is NUL terminated.
pub(crate) unsafe extern "C" fn wcscpy_impl(dst: *mut u32, src: *const u32) -> *mut u32 {
    if dst.is_null() || src.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's string contracts.
    let source = unsafe { slice(src) };
    // SAFETY: the caller granted the destination length.
    let target = unsafe { core::slice::from_raw_parts_mut(dst, source.len().saturating_add(1)) };
    let copied = wstring::wcscpy(target, source);
    if let Some(term) = target.get_mut(copied) {
        *term = 0;
    }
    dst
}

/// `wcsncpy`.
///
/// # Safety
/// `dst` is writable for `n` units; `src` is NUL terminated.
pub(crate) unsafe extern "C" fn wcsncpy_impl(dst: *mut u32, src: *const u32, n: usize) -> *mut u32 {
    if dst.is_null() || src.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's contracts.
    let source = unsafe { slice(src) };
    // SAFETY: the caller granted `n` writable units.
    let target = unsafe { core::slice::from_raw_parts_mut(dst, n) };
    let copied = wstring::wcsncpy(target, source, n);
    if let Some(rest) = target.get_mut(copied..) {
        rest.fill(0);
    }
    dst
}

/// `wcscat`.
///
/// # Safety
/// `dst` is NUL terminated and writable for both strings; `src` is NUL terminated.
pub(crate) unsafe extern "C" fn wcscat_impl(dst: *mut u32, src: *const u32) -> *mut u32 {
    if dst.is_null() || src.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's contracts.
    let source = unsafe { slice(src) };
    let used = unsafe { len(dst) };
    // SAFETY: the destination has room for both strings and a NUL.
    let target = unsafe {
        core::slice::from_raw_parts_mut(dst, used.saturating_add(source.len()).saturating_add(1))
    };
    let end = wstring::wcscat(target, source);
    if let Some(term) = target.get_mut(end) {
        *term = 0;
    }
    dst
}

/// `wcsncat`.
///
/// # Safety
/// `dst` is NUL terminated and writable; `src` is NUL terminated.
pub(crate) unsafe extern "C" fn wcsncat_impl(dst: *mut u32, src: *const u32, n: usize) -> *mut u32 {
    if dst.is_null() || src.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's contracts.
    let source = unsafe { slice(src) };
    let used = unsafe { len(dst) };
    let count = n.min(source.len());
    // SAFETY: the destination has room for the appended units and a NUL.
    let target = unsafe {
        core::slice::from_raw_parts_mut(dst, used.saturating_add(count).saturating_add(1))
    };
    let end = wstring::wcsncat(target, source, n);
    if let Some(term) = target.get_mut(end) {
        *term = 0;
    }
    dst
}

/// `wcscmp`.
///
/// # Safety
/// Both arguments are NUL terminated.
pub(crate) unsafe extern "C" fn wcscmp_impl(a: *const u32, b: *const u32) -> c_int {
    // SAFETY: the caller's strings.
    let (left, right) = unsafe { (slice(a), slice(b)) };
    wstring::wcscmp(left, right)
}

/// `wcsncmp`.
///
/// # Safety
/// Both arguments are NUL terminated and readable for `n` units.
pub(crate) unsafe extern "C" fn wcsncmp_impl(a: *const u32, b: *const u32, n: usize) -> c_int {
    // SAFETY: the caller's strings.
    let (left, right) = unsafe { (slice(a), slice(b)) };
    wstring::wcsncmp(left, right, n)
}

/// `wcschr`.
///
/// # Safety
/// `s` is NUL terminated.
pub(crate) unsafe extern "C" fn wcschr_impl(s: *const u32, unit: u32) -> *mut u32 {
    // SAFETY: the caller's string.
    let text = unsafe { slice(s) };
    match wstring::wcschr(text, unit) {
        Some(index) => {
            // SAFETY: the index is inside the caller's string.
            unsafe { s.add(index).cast_mut() }
        }
        None => core::ptr::null_mut(),
    }
}

/// `wcsrchr`.
///
/// # Safety
/// `s` is NUL terminated.
pub(crate) unsafe extern "C" fn wcsrchr_impl(s: *const u32, unit: u32) -> *mut u32 {
    // SAFETY: the caller's string.
    let text = unsafe { slice(s) };
    match wstring::wcsrchr(text, unit) {
        Some(index) => {
            // SAFETY: the index is inside the caller's string.
            unsafe { s.add(index).cast_mut() }
        }
        None => core::ptr::null_mut(),
    }
}

/// `wcscspn`.
///
/// # Safety
/// Both arguments are NUL terminated.
pub(crate) unsafe extern "C" fn wcscspn_impl(s: *const u32, reject: *const u32) -> usize {
    // SAFETY: the caller's strings.
    let (text, reject) = unsafe { (slice(s), slice(reject)) };
    wstring::wcscspn(text, reject)
}

/// `wcsspn`.
///
/// # Safety
/// Both arguments are NUL terminated.
pub(crate) unsafe extern "C" fn wcsspn_impl(s: *const u32, accept: *const u32) -> usize {
    // SAFETY: the caller's strings.
    let (text, accept) = unsafe { (slice(s), slice(accept)) };
    wstring::wcsspn(text, accept)
}

/// `wcspbrk`.
///
/// # Safety
/// Both arguments are NUL terminated.
pub(crate) unsafe extern "C" fn wcspbrk_impl(s: *const u32, accept: *const u32) -> *mut u32 {
    // SAFETY: the caller's strings.
    let (text, accept) = unsafe { (slice(s), slice(accept)) };
    match wstring::wcspbrk(text, accept) {
        Some(index) => {
            // SAFETY: the index is inside the caller's string.
            unsafe { s.add(index).cast_mut() }
        }
        None => core::ptr::null_mut(),
    }
}

/// `wcsstr`.
///
/// # Safety
/// Both arguments are NUL terminated.
pub(crate) unsafe extern "C" fn wcsstr_impl(haystack: *const u32, needle: *const u32) -> *mut u32 {
    // SAFETY: the caller's strings.
    let (haystack_slice, needle_slice) = unsafe { (slice(haystack), slice(needle)) };
    match wstring::wcsstr(haystack_slice, needle_slice) {
        Some(index) => {
            // SAFETY: the index is inside the caller's string.
            unsafe { haystack.add(index).cast_mut() }
        }
        None => core::ptr::null_mut(),
    }
}

/// `wmemchr`.
///
/// # Safety
/// `s` is readable for `n` units.
pub(crate) unsafe extern "C" fn wmemchr_impl(s: *const u32, unit: u32, n: usize) -> *mut u32 {
    if s.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller granted `n` readable units.
    let text = unsafe { core::slice::from_raw_parts(s, n) };
    match wstring::wmemchr(text, unit) {
        Some(index) => unsafe { s.add(index).cast_mut() },
        None => core::ptr::null_mut(),
    }
}

/// `wmemcmp`.
///
/// # Safety
/// Both arguments are readable for `n` units.
pub(crate) unsafe extern "C" fn wmemcmp_impl(a: *const u32, b: *const u32, n: usize) -> c_int {
    if a.is_null() || b.is_null() {
        return 0;
    }
    // SAFETY: the caller granted `n` readable units in each array.
    let (left, right) = unsafe {
        (
            core::slice::from_raw_parts(a, n),
            core::slice::from_raw_parts(b, n),
        )
    };
    wstring::wmemcmp(left, right, n)
}

/// `wmemcpy`.
///
/// # Safety
/// `dst` is writable and `src` readable for `n` units; the arrays do not overlap.
pub(crate) unsafe extern "C" fn wmemcpy_impl(dst: *mut u32, src: *const u32, n: usize) -> *mut u32 {
    if dst.is_null() || src.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's non-overlapping array contract.
    let (target, source) = unsafe {
        (
            core::slice::from_raw_parts_mut(dst, n),
            core::slice::from_raw_parts(src, n),
        )
    };
    wstring::wmemcpy(target, source);
    dst
}

/// `wmemmove`.
///
/// # Safety
/// `dst` is writable and `src` readable for `n` units; overlap is allowed.
pub(crate) unsafe extern "C" fn wmemmove_impl(
    dst: *mut u32,
    src: *const u32,
    n: usize,
) -> *mut u32 {
    if dst.is_null() || src.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's array contract; the helper handles overlap.
    let (target, source) = unsafe {
        (
            core::slice::from_raw_parts_mut(dst, n),
            core::slice::from_raw_parts(src, n),
        )
    };
    wstring::wmemmove(target, source);
    dst
}

/// `wmemset`.
///
/// # Safety
/// `dst` is writable for `n` units.
pub(crate) unsafe extern "C" fn wmemset_impl(dst: *mut u32, unit: u32, n: usize) -> *mut u32 {
    if dst.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller granted `n` writable units.
    let target = unsafe { core::slice::from_raw_parts_mut(dst, n) };
    wstring::wmemset(target, unit, n);
    dst
}

/// `wcscasecmp`.
///
/// # Safety
/// Both arguments are NUL terminated.
pub(crate) unsafe extern "C" fn wcscasecmp_impl(a: *const u32, b: *const u32) -> c_int {
    // SAFETY: the caller's strings.
    let (left, right) = unsafe { (slice(a), slice(b)) };
    raven_libc_locale::casecmp::wcscasecmp(left, right)
}

/// `wcsncasecmp`.
///
/// # Safety
/// Both arguments are NUL terminated and readable for `n` units.
pub(crate) unsafe extern "C" fn wcsncasecmp_impl(a: *const u32, b: *const u32, n: usize) -> c_int {
    // SAFETY: the caller's strings.
    let (left, right) = unsafe { (slice(a), slice(b)) };
    raven_libc_locale::casecmp::wcsncasecmp(left, right, n)
}

/// `wcslcpy` (OpenBSD).
///
/// # Safety
/// `dst` is writable for `size` units; `src` is NUL terminated.
pub(crate) unsafe extern "C" fn wcslcpy_impl(dst: *mut u32, src: *const u32, size: usize) -> usize {
    if dst.is_null() || src.is_null() {
        return 0;
    }
    // SAFETY: the caller's string and buffer contracts.
    let (target, source) = unsafe { (core::slice::from_raw_parts_mut(dst, size), slice(src)) };
    wstring::wcslcpy(target, source)
}

/// `wcslcat` (OpenBSD).
///
/// # Safety
/// `dst` is a NUL-terminated writable buffer of `size` units; `src` is NUL terminated.
pub(crate) unsafe extern "C" fn wcslcat_impl(dst: *mut u32, src: *const u32, size: usize) -> usize {
    if dst.is_null() || src.is_null() {
        return 0;
    }
    // SAFETY: the caller's string and buffer contracts.
    let (target, source) = unsafe { (core::slice::from_raw_parts_mut(dst, size), slice(src)) };
    wstring::wcslcat(target, source)
}

/// `wcswidth`.
///
/// # Safety
/// `s` is NUL terminated.
pub(crate) unsafe extern "C" fn wcswidth_impl(s: *const u32, n: usize) -> c_int {
    if s.is_null() {
        return -1;
    }
    // SAFETY: the caller granted `n` readable units.
    let text = unsafe { core::slice::from_raw_parts(s, n) };
    raven_libc_ctype::wcswidth(text)
}

/// Keep the imported `c_void` type from being flagged as unused on some configurations.
#[allow(dead_code)]
const _: *const c_void = core::ptr::null();

mod wcslcpy;
mod wcslen;
mod wcsncat;
mod wcsrchr;
mod wcsstr;
mod wmemmove;
