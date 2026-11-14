// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `jrand48(3)`.
use core::ffi::c_long;

/// A signed `long` from the caller's seed.
///
/// # Safety
/// `seed` points to a writable `unsigned short[3]`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jrand48(seed: *mut u16) -> c_long {
    if seed.is_null() {
        return 0;
    }
    // SAFETY: the caller's three-word seed.
    let words = unsafe { &mut *seed.cast::<[u16; 3]>() };
    raven_libc_stdlib::rand::jrand48(crate::stdlib::rand_state::generator48(), words)
}
