// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_setspecific` (thread.h).

use core::ffi::{c_int, c_void};

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub extern "C" fn thr_setspecific(key: u32, value: *mut c_void) -> c_int {
    ret_code(thread_names::thr_setspecific(
        &mut RealThreads,
        key,
        value as usize,
    ))
}
