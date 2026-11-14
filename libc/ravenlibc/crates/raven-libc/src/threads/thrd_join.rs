// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thrd_join(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::c11;
use raven_libc_thread::raw::SysKernel;
#[unsafe(no_mangle)]
pub extern "C" fn thrd_join(thread: usize, result: *mut c_int) -> c_int {
    match c11::thrd_join(&raven_libc_thread::raw::SysKernel, thread) {
        Ok(value) => {
            if !result.is_null() {
                unsafe { *result = value as c_int };
            }
            0
        }
        Err(code) => code,
    }
}
