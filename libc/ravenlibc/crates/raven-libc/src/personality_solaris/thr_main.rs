// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_main` (thread.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::RealThreads;

#[unsafe(no_mangle)]
pub extern "C" fn thr_main() -> c_int {
    thread_names::thr_main(&RealThreads)
}
