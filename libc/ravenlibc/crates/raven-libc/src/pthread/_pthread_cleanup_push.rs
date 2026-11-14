// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `_pthread_cleanup_push(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::cancel;
#[unsafe(no_mangle)]
pub extern "C" fn _pthread_cleanup_push(
    node: *mut cancel::Cleanup,
    function: Option<extern "C" fn(*mut core::ffi::c_void)>,
    arg: *mut core::ffi::c_void,
) {
    if node.is_null() {
        return;
    }
    let Some(function) = function else {
        return;
    };
    let function: fn(usize) = unsafe {
        core::mem::transmute::<extern "C" fn(*mut core::ffi::c_void), fn(usize)>(function)
    };
    unsafe {
        (*node).function = function;
        (*node).arg = arg as usize;
    }
    cancel::push_cleanup(&SysKernel, unsafe { &mut *node });
}
