// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mtx_timedlock(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::{c11, mutex::Mutex};
#[unsafe(no_mangle)]
pub extern "C" fn mtx_timedlock(
    mutex: *mut Mutex,
    at: *const raven_libc_abi::structs::Timespec,
) -> c_int {
    if mutex.is_null() || at.is_null() {
        return 2;
    }
    c11::mtx_timedlock(&SysKernel, unsafe { &*mutex }, unsafe { *at }) // SAFETY: the null checks above guarantee a live mutex and a readable timespec.
}
