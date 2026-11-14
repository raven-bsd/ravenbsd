// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tss_create(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::c11;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::once::Once;
#[unsafe(no_mangle)]
pub extern "C" fn tss_create(
    key: *mut u32,
    destructor: Option<extern "C" fn(*mut c_void)>,
) -> c_int {
    if key.is_null() {
        return 2;
    }
    let dtor: Option<fn(usize)> = destructor
        .map(|f| unsafe { core::mem::transmute::<extern "C" fn(*mut c_void), fn(usize)>(f) });
    let (code, value) = c11::tss_create(&SysKernel, dtor);
    if code == 0 {
        unsafe { *key = value };
    }
    code
}
