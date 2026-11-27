// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_setspecific(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_abi::types::pthread_key_t;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::tsd;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_setspecific(key: pthread_key_t, value: *const c_void) -> c_int {
    match tsd::setspecific(&SysKernel, key as u32, value as usize) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
