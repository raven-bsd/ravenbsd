// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_tryjoin_np(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::{self, attr as a};
#[unsafe(no_mangle)]
pub extern "C" fn pthread_tryjoin_np(thread: usize, out: *mut *mut c_void) -> c_int {
    match thread::tryjoin(&raven_libc_thread::raw::SysKernel, thread) {
        Ok(value) => {
            if !out.is_null() {
                unsafe { *out = value as *mut c_void }; // SAFETY: out is non-NULL after the check and writable for one thread return value.
            }
            0
        }
        Err(e) => super::common::err(e),
    }
}
