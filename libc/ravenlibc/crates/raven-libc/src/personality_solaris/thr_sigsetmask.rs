// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_sigsetmask` (thread.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::seams::SigSet;
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn thr_sigsetmask(how: c_int, set: *const SigSet, old: *mut SigSet) -> c_int {
    let set = if set.is_null() {
        None
    } else {
        // SAFETY: `set` is non-NULL and readable (C prototype).
        Some(unsafe { &*set })
    };
    let old = if old.is_null() {
        None
    } else {
        // SAFETY: `old` is non-NULL and writable (C prototype).
        Some(unsafe { &mut *old })
    };
    ret_code(thread_names::thr_sigsetmask(
        &mut RealThreads,
        how,
        set,
        old,
    ))
}
