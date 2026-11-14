// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_getconcurrency` (thread.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::concurrency;

#[unsafe(no_mangle)]
pub extern "C" fn thr_getconcurrency() -> c_int {
    let hint = concurrency();
    thread_names::thr_getconcurrency(&hint)
}
