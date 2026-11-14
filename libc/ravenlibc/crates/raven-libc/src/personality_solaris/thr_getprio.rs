// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_getprio` (thread.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn thr_getprio(thread: usize, priority: *mut c_int) -> c_int {
    if priority.is_null() {
        return ret_code(raven_libc_abi::errno::EINVAL);
    }
    let mut value = 0i32;
    let result = thread_names::thr_getprio(&mut RealThreads, thread, &mut value);
    if result == 0 {
        // SAFETY: `priority` is non-NULL and writable (C prototype).
        unsafe { *priority = value };
    }
    ret_code(result)
}
