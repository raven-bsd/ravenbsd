// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mutex_unlock` (synch.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::seams::MutexT;
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mutex_unlock(object: *mut MutexT) -> c_int {
    // SAFETY: the caller passed a writable `MutexT` (C prototype).
    let object = unsafe { &mut *object };
    ret_code(thread_names::mutex_unlock(&mut RealThreads, object))
}
