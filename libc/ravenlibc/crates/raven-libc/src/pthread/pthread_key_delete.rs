// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_key_delete(3)`.
use core::ffi::c_int;
use raven_libc_abi::types::pthread_key_t;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::tsd;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_key_delete(key: pthread_key_t) -> c_int {
    tsd::key_delete(&SysKernel, key as u32);
    0
}
