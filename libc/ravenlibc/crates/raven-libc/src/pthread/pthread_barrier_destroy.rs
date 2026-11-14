// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_barrier_destroy(3)`.
use core::ffi::c_int;
use raven_libc_thread::barrier as b;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_barrier_destroy(barrier: *mut b::Barrier) -> c_int {
    if barrier.is_null() {
        return 22;
    }
    // SAFETY: barrier is non-NULL after the check and points to a live initialised barrier.
    match b::destroy(&raven_libc_thread::raw::SysKernel, unsafe { &*barrier }) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
