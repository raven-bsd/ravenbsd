// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `cond_wait` (synch.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::seams::{CondT, MutexT};
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cond_wait(cond: *mut CondT, mutex: *mut MutexT) -> c_int {
    // SAFETY: the caller passed two writable objects (C prototype).
    let cond = unsafe { &mut *cond };
    // SAFETY: the caller passed two writable objects (C prototype).
    let mutex = unsafe { &mut *mutex };
    ret_code(thread_names::cond_wait(&mut RealThreads, cond, mutex))
}
