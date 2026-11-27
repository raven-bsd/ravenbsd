// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_spin_trylock(3)`.
use core::ffi::c_int;
use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::types::pthread_spinlock_t;
use raven_libc_thread::spin as s;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_spin_trylock(lock: *mut pthread_spinlock_t) -> c_int {
    let Some(object) = super::common::spin_object(lock) else {
        return EINVAL;
    };
    // SAFETY: spin_object returned the live object for this slot.
    match s::trylock(unsafe { &*object }) {
        Ok(()) => 0,
        Err(error) => super::common::err(error),
    }
}
