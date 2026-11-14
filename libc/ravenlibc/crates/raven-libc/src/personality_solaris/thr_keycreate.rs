// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_keycreate` (thread.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::seams::KeyDtor;
use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{RealThreads, ret_code};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn thr_keycreate(key: *mut u32, dtor: Option<KeyDtor>) -> c_int {
    if key.is_null() {
        return ret_code(raven_libc_abi::errno::EINVAL);
    }
    let mut created = 0u32;
    let result = thread_names::thr_keycreate(&mut RealThreads, dtor, &mut created);
    if result == 0 {
        // SAFETY: `key` is non-NULL and writable (C prototype).
        unsafe { *key = created };
    }
    ret_code(result)
}
