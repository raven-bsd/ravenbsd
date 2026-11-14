// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_setprio` (thread.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub extern "C" fn thr_setprio(thread: usize, priority: c_int) -> c_int {
    ret_code(thread_names::thr_setprio(
        &mut RealThreads,
        thread,
        priority,
    ))
}
