// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `regnsub` and `regasub` from `<regex.h>` over C45's substitution engine.

use alloc::vec::Vec;
use core::ffi::c_char;

use raven_libc_abi::errno::Errno;
use raven_libc_regex::RegMatch;

use super::RegMatchC;

/// The number of `\1`..`\9` references the engine consults.
const MATCHES: usize = 10;

/// Read the caller's match array.
///
/// # Safety
/// `pmatch` has at least `MATCHES` readable entries.
unsafe fn matches(pmatch: *const RegMatchC) -> Vec<RegMatch> {
    let mut out = Vec::with_capacity(MATCHES);
    for index in 0..MATCHES {
        // SAFETY: the caller's array.
        let value = unsafe { &*pmatch.add(index) };
        out.push(RegMatch {
            rm_so: value.rm_so,
            rm_eo: value.rm_eo,
        });
    }
    out
}

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

/// `regasub`.
///
/// # Safety
/// `out` is writable; `sub`, `pmatch` and `source` follow the C contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regasub(
    out: *mut *mut c_char,
    sub: *const c_char,
    pmatch: *const RegMatchC,
    source: *const c_char,
) -> isize {
    if out.is_null() || sub.is_null() || pmatch.is_null() || source.is_null() {
        crate::ffi::errno::set(Errno(14));
        return -1;
    }
    // SAFETY: the caller's strings.
    let (sub, matches, source) = unsafe {
        (
            crate::ffi::cstr::slice(sub),
            matches(pmatch),
            crate::ffi::cstr::slice(source),
        )
    };
    match raven_libc_regex::regasub(sub, &matches, source) {
        Ok(bytes) => {
            let length = bytes.len();
            // SAFETY: the shell allocator; the caller frees with `free`.
            let block =
                unsafe { crate::malloc::abi_alloc(length.saturating_add(1)) } as *mut c_char;
            if block.is_null() {
                crate::ffi::errno::set(Errno(12));
                return -1;
            }
            // SAFETY: the block has `length + 1` bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), block.cast::<u8>(), length);
                *block.cast::<u8>().add(length) = 0;
                *out = block;
            }
            isize::try_from(length).unwrap_or(isize::MAX)
        }
        Err(_) => {
            crate::ffi::errno::set(Errno(12));
            -1
        }
    }
}
