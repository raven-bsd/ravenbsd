// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mtx_init(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::{c11, mutex::Mutex};
#[unsafe(no_mangle)]
pub extern "C" fn mtx_init(mutex: *mut Mutex, kind: c_int) -> c_int {
    if mutex.is_null() {
        return 2;
    }
    c11::mtx_init(unsafe { &mut *mutex }, kind) // SAFETY: mutex is non-NULL after the check and exclusively borrowed for initialisation.
}
