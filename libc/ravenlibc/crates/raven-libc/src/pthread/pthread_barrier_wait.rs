// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_barrier_wait(3)`.
use core::ffi::c_int;
use raven_libc_thread::barrier as b;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_barrier_wait(barrier: *mut b::Barrier) -> c_int {
    if barrier.is_null() {
        return 22;
    }
    match b::wait_barrier(&raven_libc_thread::raw::SysKernel, unsafe { &*barrier }) {
        Ok(value) => value,
        Err(e) => super::common::err(e),
    }
}
