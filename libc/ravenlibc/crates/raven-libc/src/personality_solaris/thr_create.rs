// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_create` (thread.h).

use core::ffi::{c_int, c_long, c_void};

use raven_libc_personality_solaris::seams::StartFn;
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

type CStart = extern "C" fn(*mut c_void) -> *mut c_void;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn thr_create(
    stack_base: *mut c_void,
    stack_size: usize,
    start: Option<CStart>,
    arg: *mut c_void,
    flags: c_long,
    new_thread: *mut usize,
) -> c_int {
    let Some(start) = start else {
        return ret_code(raven_libc_abi::errno::EINVAL);
    };
    // SAFETY: `CStart` and `StartFn` are both plain function pointers; the argument and result
    // are one machine word each, and the shell is the documented adapter (C67 4.2).
    let start: StartFn = unsafe { core::mem::transmute(start) };
    let out = if new_thread.is_null() {
        None
    } else {
        // SAFETY: `new_thread` is non-NULL and writable (C prototype).
        Some(unsafe { &mut *new_thread })
    };
    ret_code(thread_names::thr_create(
        &mut RealThreads,
        stack_base as usize,
        stack_size,
        start,
        arg as usize,
        flags,
        out,
    ))
}
