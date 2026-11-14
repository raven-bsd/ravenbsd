// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_self` (thread.h).

use raven_libc_personality_solaris::thread_names;

use crate::personality_solaris::real::RealThreads;

#[unsafe(no_mangle)]
pub extern "C" fn thr_self() -> usize {
    thread_names::thr_self(&RealThreads)
}
