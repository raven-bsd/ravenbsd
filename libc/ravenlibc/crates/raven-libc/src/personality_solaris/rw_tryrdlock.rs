// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `rw_tryrdlock` (synch.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::seams::RwlockT;
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rw_tryrdlock(object: *mut RwlockT) -> c_int {
    // SAFETY: the caller passed a writable `RwlockT` (C prototype).
    let object = unsafe { &mut *object };
    ret_code(thread_names::rw_tryrdlock(&mut RealThreads, object))
}
