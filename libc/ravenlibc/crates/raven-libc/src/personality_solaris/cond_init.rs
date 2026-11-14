// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `cond_init` (synch.h).

use core::ffi::{c_int, c_void};

use raven_libc_personality_solaris::seams::CondT;
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cond_init(object: *mut CondT, kind: c_int, arg: *mut c_void) -> c_int {
    // SAFETY: the caller passed a writable `CondT` (C prototype).
    let object = unsafe { &mut *object };
    ret_code(thread_names::cond_init(
        &mut RealThreads,
        object,
        kind,
        arg.is_null(),
    ))
}
