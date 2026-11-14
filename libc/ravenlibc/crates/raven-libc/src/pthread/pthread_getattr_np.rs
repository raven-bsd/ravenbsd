// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_getattr_np(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::{self, attr as a};
#[unsafe(no_mangle)]
pub extern "C" fn pthread_getattr_np(thread: usize, out: *mut a::Attr) -> c_int {
    if out.is_null() || thread == 0 {
        return 22;
    }
    // SAFETY: the handle is a live TCB pointer; getattr_np only reads it.
    unsafe { *out = a::getattr_np(&*(thread as *const raven_libc_tls::Pthread)) };
    0
}
