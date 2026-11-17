// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `tzname[2]` data symbol.

use core::ffi::c_char;

/// POSIX `tzname[2]`; `zone::publish` replaces the pointers after each `TZ` reload.
#[unsafe(no_mangle)]
pub static mut tzname: [*mut c_char; 2] = [core::ptr::null_mut(); 2];

/// Publish the two abbreviation pointers.
pub(crate) fn publish(pointers: [*mut c_char; 2]) {
    // SAFETY: the shell is single-threaded and the pointers borrow the cache until the next
    // reload; the C contract allows `tzset` to replace them.
    unsafe {
        tzname = pointers;
    }
}
