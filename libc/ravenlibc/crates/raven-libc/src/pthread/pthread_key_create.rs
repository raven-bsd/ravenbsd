// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_key_create(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_abi::types::pthread_key_t;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::tsd;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_key_create(
    key: *mut pthread_key_t,
    destructor: Option<extern "C" fn(*mut c_void)>,
) -> c_int {
    if key.is_null() {
        return 22;
    }
    let dtor: Option<fn(usize)> = destructor
        .map(|f| unsafe { core::mem::transmute::<extern "C" fn(*mut c_void), fn(usize)>(f) }); // SAFETY: both one-argument fn types share the C ABI; the destructor is called with the thread value.
    match tsd::key_create(&SysKernel, dtor) {
        Ok(value) => {
            unsafe { *key = value as pthread_key_t }; // SAFETY: key is non-NULL after the check and writable for one key value.
            0
        }
        Err(e) => super::common::err(e),
    }
}
