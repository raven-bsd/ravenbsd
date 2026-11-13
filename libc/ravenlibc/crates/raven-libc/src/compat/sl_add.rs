// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the sl_add export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `sl_add` C-ABI export (ticket C65).

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

/// `sl_add`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sl_add(list: *mut CStringList, name: *mut c_char) -> c_int {
    if list.is_null() {
        return -1;
    }
    // SAFETY: the caller passes a live StringList from `sl_init`.
    let (mut current, mut maximum, mut strings) =
        unsafe { ((*list).sl_cur, (*list).sl_max, (*list).sl_str) };
    if current.saturating_add(1) >= maximum {
        maximum = maximum.saturating_add(SL_CHUNKSIZE);
        // SAFETY: `strings` is the array allocated in `sl_init`/`sl_add` with `maximum` entries.
        let grown = unsafe {
            super::realloc(
                strings.cast(),
                maximum * core::mem::size_of::<*mut c_char>(),
            )
        }
        .cast::<*mut c_char>();
        if grown.is_null() {
            // The donor's reallocf frees the old block on failure.
            // SAFETY: `strings` is the old allocation and is no longer used.
            unsafe { super::free(strings.cast()) };
            // SAFETY: `list` is live.
            unsafe { (*list).sl_str = core::ptr::null_mut() };
            return -1;
        }
        strings = grown;
        // SAFETY: `list` is live.
        unsafe {
            (*list).sl_max = maximum;
            (*list).sl_str = strings;
        }
    }
    // SAFETY: `current + 1 < maximum` and `strings` has `maximum` slots.
    unsafe {
        *strings.add(current) = name;
        (*list).sl_cur = current + 1;
    }
    0
}
