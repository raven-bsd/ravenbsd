// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thrd_create(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::c11;
use raven_libc_thread::raw::SysKernel;
#[unsafe(no_mangle)]
pub extern "C" fn thrd_create(
    thread: *mut usize,
    start: Option<extern "C" fn(*mut c_void) -> c_int>,
    arg: *mut c_void,
) -> c_int {
    if thread.is_null() {
        return 2;
    }
    let Some(start) = start else {
        return 2;
    };
    struct StartInfo {
        start: extern "C" fn(*mut c_void) -> c_int,
        arg: *mut c_void,
    }
    fn trampoline(info: usize) -> usize {
        // SAFETY: the box was created below and is owned by this thread entry.
        let info = unsafe { alloc::boxed::Box::from_raw(info as *mut StartInfo) };
        (info.start)(info.arg) as usize
    }
    let info = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(StartInfo { start, arg }));
    match c11::thrd_create(&SysKernel, trampoline, info as usize) {
        Ok(value) => {
            unsafe { *thread = value }; // SAFETY: thread is non-NULL after the check and writable for one thrd_t.
            0
        }
        Err(code) => {
            unsafe {
                // SAFETY: info came from Box::into_raw above and thrd_create failed, so this reclaims the only owning pointer.
                drop(alloc::boxed::Box::from_raw(info));
            };
            code
        }
    }
}
