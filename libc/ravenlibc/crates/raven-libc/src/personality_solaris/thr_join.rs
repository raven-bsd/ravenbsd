// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_join` (thread.h).

use core::ffi::{c_int, c_void};

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn thr_join(
    wait_for: usize,
    departed: *mut usize,
    status: *mut *mut c_void,
) -> c_int {
    let mut value = 0usize;
    let status_out = if status.is_null() {
        None
    } else {
        Some(&mut value)
    };
    let departed_out = if departed.is_null() {
        None
    } else {
        // SAFETY: `departed` is non-NULL and writable (C prototype).
        Some(unsafe { &mut *departed })
    };
    let result = thread_names::thr_join(&mut RealThreads, wait_for, departed_out, status_out);
    if result == 0 && !status.is_null() {
        // SAFETY: `status` is non-NULL and writable (C prototype).
        unsafe { *status = value as *mut c_void };
    }
    ret_code(result)
}
