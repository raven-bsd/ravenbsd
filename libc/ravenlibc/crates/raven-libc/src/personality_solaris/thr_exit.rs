// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_exit` (thread.h).

use core::ffi::c_void;

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::RealThreads;

#[unsafe(no_mangle)]
pub extern "C" fn thr_exit(status: *mut c_void) -> ! {
    thread_names::thr_exit(&mut RealThreads, status as usize)
}
