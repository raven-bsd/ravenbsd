// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `erand48(3)`.
/// A double from the caller's three-word seed.
///
/// # Safety
/// `seed` points to a writable `unsigned short[3]`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn erand48(seed: *mut u16) -> f64 {
    if seed.is_null() {
        return 0.0;
    }
    // SAFETY: the caller's three-word seed.
    let words = unsafe { &mut *seed.cast::<[u16; 3]>() };
    raven_libc_stdlib::rand::erand48(crate::stdlib::rand_state::generator48(), words)
}
