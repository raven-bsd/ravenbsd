// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `rand_r(3)`.
use core::ffi::c_int;

/// A reentrant pseudo-random number from the caller's seed.
///
/// # Safety
/// `seed` points to a writable `unsigned int`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rand_r(seed: *mut u32) -> c_int {
    if seed.is_null() {
        return 0;
    }
    // SAFETY: the caller's seed slot.
    raven_libc_stdlib::rand::rand_r(unsafe { &mut *seed }) as c_int
}
