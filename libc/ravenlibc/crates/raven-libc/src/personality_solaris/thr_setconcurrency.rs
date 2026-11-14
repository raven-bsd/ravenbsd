// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_setconcurrency` (thread.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::{concurrency, ret_code, set_concurrency};

#[unsafe(no_mangle)]
pub extern "C" fn thr_setconcurrency(level: c_int) -> c_int {
    let mut hint = concurrency();
    let result = thread_names::thr_setconcurrency(&mut hint, level);
    if result == 0 {
        set_concurrency(hint);
    }
    ret_code(result)
}
