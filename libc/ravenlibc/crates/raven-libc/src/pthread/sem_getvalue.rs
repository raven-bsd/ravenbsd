// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_getvalue(3)`.
use core::ffi::c_int;
use raven_libc_thread::sem as s;
#[unsafe(no_mangle)]
pub extern "C" fn sem_getvalue(sem: *mut s::Sem, out: *mut c_int) -> c_int {
    if sem.is_null() || out.is_null() {
        return 22;
    }
    unsafe { *out = s::getvalue(&*sem) }; // SAFETY: the null checks above guarantee a readable semaphore and a writable out slot.
    0
}
