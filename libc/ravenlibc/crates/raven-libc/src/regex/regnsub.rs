// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `regnsub` from `<regex.h>`.

use core::ffi::c_char;

use raven_libc_abi::errno::Errno;

use super::RegMatchC;
use super::regsub::matches;

/// `regnsub`.
///
/// # Safety
/// `buf` has `size` writable bytes; `sub`, `pmatch` and `source` follow the C contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regnsub(
    buf: *mut c_char,
    size: usize,
    sub: *const c_char,
    pmatch: *const RegMatchC,
    source: *const c_char,
) -> isize {
    if buf.is_null() || sub.is_null() || pmatch.is_null() || source.is_null() {
        crate::ffi::errno::set(Errno(14));
        return -1;
    }
    // SAFETY: the caller's buffer and strings.
    let (out, sub, matches, source) = unsafe {
        (
            core::slice::from_raw_parts_mut(buf.cast::<u8>(), size),
            crate::ffi::cstr::slice(sub),
            matches(pmatch),
            crate::ffi::cstr::slice(source),
        )
    };
    raven_libc_regex::regnsub(out, sub, &matches, source)
}
