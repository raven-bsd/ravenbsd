// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the sl_init export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `sl_init` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

/// The C `StringList` (`char **sl_str; size_t sl_max; size_t sl_cur;`).
#[repr(C)]
pub struct CStringList {
    /// `sl_str`.
    pub sl_str: *mut *mut c_char,
    /// `sl_max`.
    pub sl_max: usize,
    /// `sl_cur`.
    pub sl_cur: usize,
}

const SL_CHUNKSIZE: usize = 20;

/// `sl_init`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sl_init() -> *mut CStringList {
    // SAFETY: `malloc` returns a fresh block of the requested size or null.
    let list = unsafe { super::malloc(core::mem::size_of::<CStringList>()) }.cast::<CStringList>();
    if list.is_null() {
        return list;
    }
    // SAFETY: `list` is a fresh CStringList; the string array is `sl_max` pointers.
    let strings = unsafe { super::malloc(SL_CHUNKSIZE * core::mem::size_of::<*mut c_char>()) }
        .cast::<*mut c_char>();
    if strings.is_null() {
        // SAFETY: `list` was allocated above and is not aliased.
        unsafe { super::free(list.cast()) };
        return core::ptr::null_mut();
    }
    // SAFETY: both allocations are large enough for the three writes.
    unsafe {
        (*list).sl_cur = 0;
        (*list).sl_max = SL_CHUNKSIZE;
        (*list).sl_str = strings;
    }
    list
}
