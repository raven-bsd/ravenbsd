// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the hasmntopt export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `hasmntopt` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::mntent_state::CMntEnt;

use raven_libc_compat::mntent::hasmntopt as logic;

/// `hasmntopt`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hasmntopt(
    entry: *const CMntEnt,
    option: *const core::ffi::c_char,
) -> *mut core::ffi::c_char {
    if entry.is_null() || option.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: both strings are NUL-terminated.
    let options = unsafe { super::c_bytes((*entry).mnt_opts, super::MAX_CSTR) };
    let option = unsafe { super::c_bytes(option, super::MAX_NAME) };
    match logic(options, option) {
        // SAFETY: the offset is inside `mnt_opts`.
        Some(offset) => unsafe { (*entry).mnt_opts.add(offset) },
        None => core::ptr::null_mut(),
    }
}
