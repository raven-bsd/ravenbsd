// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memmem(3)`.
use core::ffi::c_void;

/// The first occurrence of `needle_len` bytes of `needle` in `haystack`.
///
/// # Safety
/// Both regions are readable for their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmem(
    haystack: *const c_void,
    haystack_len: usize,
    needle: *const c_void,
    needle_len: usize,
) -> *mut c_void {
    // SAFETY: the caller's contract is the memmem(3) contract.
    unsafe {
        raven_libc_str::raw::memmem(haystack.cast(), haystack_len, needle.cast(), needle_len).cast()
    }
}
