// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_kill` (thread.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub extern "C" fn thr_kill(thread: usize, sig: c_int) -> c_int {
    ret_code(thread_names::thr_kill(&mut RealThreads, thread, sig))
}
