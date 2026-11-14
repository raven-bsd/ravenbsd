// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `call_once(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::c11;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::once::Once;
#[unsafe(no_mangle)]
pub extern "C" fn call_once(flag: *mut Once, function: Option<extern "C" fn()>) {
    if flag.is_null() {
        return;
    }
    let Some(function) = function else {
        return;
    };
    let function: fn() = unsafe { core::mem::transmute::<extern "C" fn(), fn()>(function) }; // SAFETY: both fn() types share the C ABI; the callback is only invoked as the once routine.
    c11::call_once(&SysKernel, unsafe { &*flag }, function); // SAFETY: flag is non-NULL after the check and points to the caller's live once flag.
}
