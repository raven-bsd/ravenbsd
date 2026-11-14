// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `_pthread_cleanup_pop(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::cancel;
#[unsafe(no_mangle)]
pub extern "C" fn _pthread_cleanup_pop(node: *mut cancel::Cleanup, run: core::ffi::c_int) {
    if node.is_null() {
        return;
    }
    cancel::pop_cleanup(&SysKernel, unsafe { &mut *node }, run != 0);
}
