// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_yield(3)` / `sched_yield(3)`.
/// Yield the CPU to another thread.
#[unsafe(no_mangle)]
pub extern "C" fn pthread_yield() -> i32 {
    let _ = raven_libc_sys::sys::yield_now();
    0
}
