// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_spin_init(3)`.
use core::alloc::Layout;
use core::ffi::c_int;
use raven_libc_abi::errno::{EINVAL, ENOMEM};
use raven_libc_abi::types::pthread_spinlock_t;
use raven_libc_thread::spin as s;

/// The allocation of one internal spinlock object.
pub(crate) const SPIN_LAYOUT: Layout = Layout::new::<s::SpinLock>();

#[unsafe(no_mangle)]
pub extern "C" fn pthread_spin_init(lock: *mut pthread_spinlock_t, shared: c_int) -> c_int {
    if lock.is_null() {
        return EINVAL;
    }
    let mut fresh = s::SpinLock::new();
    if let Err(error) = s::init(&mut fresh, shared) {
        return super::common::err(error);
    }
    // FreeBSD's pthread_spin_init allocates the opaque object and stores its pointer.
    // SAFETY: the layout is non-zero, and a null result is checked before use.
    let raw = unsafe { alloc::alloc::alloc(SPIN_LAYOUT) }.cast::<s::SpinLock>();
    if raw.is_null() {
        return ENOMEM;
    }
    // SAFETY: raw is a fresh allocation of one SpinLock and the caller's slot is writable.
    unsafe {
        raw.write(fresh);
        *lock = raw.cast::<core::ffi::c_void>();
    }
    0
}
