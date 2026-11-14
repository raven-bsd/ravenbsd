// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thr_min_stack` (thread.h).

use raven_libc_personality_solaris::thread_names;

#[unsafe(no_mangle)]
pub extern "C" fn thr_min_stack() -> usize {
    thread_names::thr_min_stack()
}
