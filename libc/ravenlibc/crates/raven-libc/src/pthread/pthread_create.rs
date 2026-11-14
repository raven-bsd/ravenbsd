// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_create(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::{self, attr as a};
#[unsafe(no_mangle)]
pub extern "C" fn pthread_create(
    thread: *mut usize,
    attr: *const a::Attr,
    start: Option<extern "C" fn(*mut c_void) -> *mut c_void>,
    arg: *mut c_void,
) -> c_int {
    if thread.is_null() {
        return 22;
    }
    let Some(start) = start else {
        return 22;
    };
    struct StartInfo {
        start: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    }
    fn trampoline(info: usize) -> usize {
        // SAFETY: the box was created below and is owned by this thread entry.
        let info = unsafe { alloc::boxed::Box::from_raw(info as *mut StartInfo) };
        (info.start)(info.arg) as usize
    }
    let info = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(StartInfo { start, arg }));
    let a = if attr.is_null() {
        None
    } else {
        Some(unsafe { &*attr })
    };
    match thread::create(&SysKernel, a, trampoline, info as usize) {
        Ok(value) => {
            unsafe { *thread = value };
            0
        }
        Err(error) => {
            unsafe {
                drop(alloc::boxed::Box::from_raw(info));
            };
            super::common::err(error)
        }
    }
}
