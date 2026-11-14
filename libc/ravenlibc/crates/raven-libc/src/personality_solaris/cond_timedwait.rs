// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `cond_timedwait` (synch.h).

use core::ffi::c_int;

use raven_libc_abi::structs::Timespec;
use raven_libc_personality_solaris::seams::{CondT, MutexT};
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cond_timedwait(
    cond: *mut CondT,
    mutex: *mut MutexT,
    abstime: *const Timespec,
) -> c_int {
    if abstime.is_null() {
        return ret_code(raven_libc_abi::errno::EINVAL);
    }
    // SAFETY: the caller passed two writable objects (C prototype).
    let cond = unsafe { &mut *cond };
    // SAFETY: the caller passed two writable objects (C prototype).
    let mutex = unsafe { &mut *mutex };
    // SAFETY: `abstime` is non-NULL and readable (C prototype).
    let abstime = unsafe { &*abstime };
    ret_code(thread_names::cond_timedwait(
        &mut RealThreads,
        cond,
        mutex,
        abstime,
    ))
}
