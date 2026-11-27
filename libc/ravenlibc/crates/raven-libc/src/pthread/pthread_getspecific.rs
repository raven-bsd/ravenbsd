// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_getspecific(3)`.
use core::ffi::c_void;
use raven_libc_abi::types::pthread_key_t;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::tsd;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_getspecific(key: pthread_key_t) -> *mut c_void {
    tsd::getspecific(&SysKernel, key as u32) as *mut c_void
}
