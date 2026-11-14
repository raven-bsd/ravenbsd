// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sema_init` (synch.h).

use core::ffi::{c_int, c_uint, c_void};

use raven_libc_personality_solaris::seams::SemaT;
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sema_init(
    sema: *mut SemaT,
    count: c_uint,
    kind: c_int,
    arg: *mut c_void,
) -> c_int {
    // SAFETY: the caller passed a writable `sema_t` (C prototype).
    let sema = unsafe { &mut *sema };
    ret_code(thread_names::sema_init(
        &mut RealThreads,
        sema,
        count,
        kind,
        arg.is_null(),
    ))
}
