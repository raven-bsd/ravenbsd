// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the sl_free export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `sl_free` C-ABI export (ticket C65).

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

/// `sl_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sl_free(list: *mut CStringList, all: c_int) {
    if list.is_null() {
        return;
    }
    // SAFETY: the caller passes a live StringList.
    let (strings, current) = unsafe { ((*list).sl_str, (*list).sl_cur) };
    if !strings.is_null() {
        if all != 0 {
            let mut index = 0usize;
            while index < current {
                // SAFETY: slot `index` was filled by `sl_add`.
                let item = unsafe { *strings.add(index) };
                if !item.is_null() {
                    // SAFETY: the item is the caller's malloc'ed string.
                    unsafe { super::free(item.cast()) };
                }
                index = index.wrapping_add(1);
            }
        }
        // SAFETY: the array is the allocation from `sl_init`.
        unsafe { super::free(strings.cast()) };
    }
    // SAFETY: `list` is the object from `sl_init`.
    unsafe { super::free(list.cast()) };
}
