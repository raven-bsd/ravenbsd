// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `lcong48(3)`.
/// Set the full seven-word generator parameter array.
///
/// # Safety
/// `parameters` points to a readable `unsigned short[7]`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lcong48(parameters: *mut u16) {
    if parameters.is_null() {
        return;
    }
    // SAFETY: the caller's seven-word parameter array.
    let words = unsafe { &*parameters.cast::<[u16; 7]>() };
    raven_libc_stdlib::rand::lcong48(crate::stdlib::rand_state::generator48(), words);
}
