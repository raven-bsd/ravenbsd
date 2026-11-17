// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_trywait(3)`.
use core::ffi::c_int;
use raven_libc_thread::sem as s;
#[unsafe(no_mangle)]
pub extern "C" fn sem_trywait(sem: *mut s::Sem) -> c_int {
    if sem.is_null() {
        return super::common::sem_err(raven_libc_thread::limits::EINVAL);
    }
    // SAFETY: the caller's contract guarantees `sem` points to a live `sem_t` for the duration
    // of the call; the null check above rejects the only invalid value.
    match s::trywait(unsafe { &*sem }) {
        Ok(()) => 0,
        Err(e) => super::common::sem_err(e),
    }
}
