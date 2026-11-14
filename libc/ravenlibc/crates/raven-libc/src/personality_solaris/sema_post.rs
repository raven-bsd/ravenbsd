// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sema_post` (synch.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::seams::SemaT;
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sema_post(object: *mut SemaT) -> c_int {
    // SAFETY: the caller passed a writable `SemaT` (C prototype).
    let object = unsafe { &mut *object };
    ret_code(thread_names::sema_post(&mut RealThreads, object))
}
