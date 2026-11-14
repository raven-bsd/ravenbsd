// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_getname_np(3)`.
use core::ffi::{c_char, c_int};
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::name;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_getname_np(thread: usize, buffer: *mut c_char, size: usize) -> c_int {
    if buffer.is_null() {
        return 22;
    }
    let out = unsafe { core::slice::from_raw_parts_mut(buffer.cast::<u8>(), size) }; // SAFETY: buffer is non-NULL after the check and size bytes are writable per the API contract.
    let length = name::getname(&SysKernel, thread, out);
    if length >= size { 34 } else { 0 }
}
