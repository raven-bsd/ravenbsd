// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the sl_find export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `sl_find` C-ABI export (ticket C65).

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

/// `sl_find`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sl_find(list: *mut CStringList, name: *const c_char) -> *mut c_char {
    if list.is_null() || name.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller passes a live StringList and a NUL-terminated name.
    let wanted = unsafe { super::c_bytes(name, super::MAX_CSTR) };
    let (strings, current) = unsafe { ((*list).sl_str, (*list).sl_cur) };
    let mut index = 0usize;
    while index < current {
        // SAFETY: slot `index` was filled by `sl_add`.
        let item = unsafe { *strings.add(index) };
        if !item.is_null() {
            // SAFETY: the item is a NUL-terminated string.
            let stored = unsafe { super::c_bytes(item, super::MAX_CSTR) };
            if stored == wanted {
                return item;
            }
        }
        index = index.wrapping_add(1);
    }
    core::ptr::null_mut()
}
