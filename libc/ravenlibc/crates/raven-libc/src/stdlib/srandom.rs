// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `srandom(3)`.
/// Seed the process `random` generator.
#[unsafe(no_mangle)]
pub extern "C" fn srandom(seed: u32) {
    crate::stdlib::rand_state::ensure_random();
    // SAFETY: the process state was initialised above.
    let state = unsafe { crate::stdlib::rand_state::state_slice() };
    raven_libc_stdlib::rand::srandom(crate::stdlib::rand_state::control(), state, seed);
}
