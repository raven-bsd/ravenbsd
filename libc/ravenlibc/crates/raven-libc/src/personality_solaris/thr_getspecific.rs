// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_getspecific` (thread.h).

use core::ffi::{c_int, c_void};

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn thr_getspecific(key: u32, value: *mut *mut c_void) -> c_int {
    if value.is_null() {
        return ret_code(raven_libc_abi::errno::EINVAL);
    }
    let mut found = 0usize;
    let result = thread_names::thr_getspecific(&mut RealThreads, key, &mut found);
    if result == 0 {
        // SAFETY: `value` is non-NULL and writable (C prototype).
        unsafe { *value = found as *mut c_void };
    }
    ret_code(result)
}
