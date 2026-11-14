// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `random(3)`.
use core::ffi::c_long;

/// A pseudo-random number in `0..=2^31-1`.
#[unsafe(no_mangle)]
pub extern "C" fn random() -> c_long {
    crate::stdlib::rand_state::ensure_random();
    // SAFETY: the process state was initialised above.
    let state = unsafe { crate::stdlib::rand_state::state_slice() };
    let value = raven_libc_stdlib::rand::random(crate::stdlib::rand_state::control(), state);
    c_long::from(value)
}
