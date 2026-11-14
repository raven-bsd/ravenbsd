// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mtx_trylock(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::{c11, mutex::Mutex};
#[unsafe(no_mangle)]
pub extern "C" fn mtx_trylock(mutex: *mut Mutex) -> c_int {
    if mutex.is_null() {
        return 2;
    }
    c11::mtx_trylock(&SysKernel, unsafe { &*mutex }) // SAFETY: mutex is non-NULL after the check and points to a live initialised mutex.
}
