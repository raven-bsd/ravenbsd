// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_rwlockattr_setpshared(3)`.
use raven_libc_thread::rwlock as r;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_rwlockattr_setpshared(
    attr: *mut r::RwLockAttr,
    value: core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    match r::attr_setpshared(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &mut *attr },
        value,
    ) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
