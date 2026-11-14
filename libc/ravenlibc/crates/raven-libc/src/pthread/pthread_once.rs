// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_once(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::once::{self, Once};
#[unsafe(no_mangle)]
pub extern "C" fn pthread_once(control: *mut Once, init: Option<extern "C" fn()>) -> c_int {
    if control.is_null() {
        return 22;
    }
    let Some(init) = init else {
        return 22;
    };
    let init: fn() = unsafe { core::mem::transmute::<extern "C" fn(), fn()>(init) }; // SAFETY: both fn() types share the C ABI; the callback is only invoked as the once routine.
    once::once(&SysKernel, unsafe { &*control }, init); // SAFETY: control is non-NULL after the check and points to the caller's live once control.
    0
}
