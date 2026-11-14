// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `seed48(3)`.
/// Set the three-word seed and return the previous one.
///
/// # Safety
/// `seed` points to a readable `unsigned short[3]`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seed48(seed: *mut u16) -> *mut u16 {
    if seed.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's three-word seed.
    let words = unsafe { &*seed.cast::<[u16; 3]>() };
    let previous = raven_libc_stdlib::rand::seed48(crate::stdlib::rand_state::generator48(), words);
    let buffer = crate::stdlib::rand_state::seed48_buffer();
    *buffer = previous;
    buffer.as_mut_ptr()
}
