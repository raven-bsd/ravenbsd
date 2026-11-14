// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_setname_np(3)`.
use core::ffi::{c_char, c_int};
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::name;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_setname_np(thread: usize, name_: *const c_char) -> c_int {
    if name_.is_null() {
        return 22;
    }
    let bytes = unsafe { crate::ffi::cstr::slice(name_) };
    match name::setname(&SysKernel, thread, bytes) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
