// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_destroy(3)`.
use core::ffi::c_int;
use raven_libc_thread::sem as s;
#[unsafe(no_mangle)]
pub extern "C" fn sem_destroy(sem: *mut s::Sem) -> c_int {
    if sem.is_null() {
        return 22;
    }
    match s::destroy(unsafe { &*sem }) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
