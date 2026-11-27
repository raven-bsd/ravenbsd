// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_spin_destroy(3)`.
use core::ffi::c_int;
use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::types::pthread_spinlock_t;
use raven_libc_thread::spin as s;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_spin_destroy(lock: *mut pthread_spinlock_t) -> c_int {
    let Some(object) = super::common::spin_object(lock) else {
        return EINVAL;
    };
    // SAFETY: spin_object returned the live object for this slot.
    s::destroy(unsafe { &*object });
    // SAFETY: the object came from pthread_spin_init's one-object allocation, freed once here.
    unsafe {
        alloc::alloc::dealloc(object.cast::<u8>(), super::pthread_spin_init::SPIN_LAYOUT);
        *lock = core::ptr::null_mut();
    }
    0
}
