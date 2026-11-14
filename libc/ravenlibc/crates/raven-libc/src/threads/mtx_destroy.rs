// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mtx_destroy(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::{c11, mutex::Mutex};
#[unsafe(no_mangle)]
pub extern "C" fn mtx_destroy(mutex: *mut Mutex) {
    if !mutex.is_null() {
        c11::mtx_destroy(unsafe { &*mutex }); // SAFETY: mutex is non-NULL after the check and points to a live initialised mutex.
    }
}
