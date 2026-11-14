// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_spin_trylock(3)`.
use core::ffi::c_int;
use raven_libc_thread::spin as s;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_spin_trylock(lock: *mut s::SpinLock) -> c_int {
    if lock.is_null() {
        return 22;
    }
    match s::trylock(unsafe { &*lock }) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
