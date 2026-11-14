// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `srand(3)`.
/// Seed the default generator.
#[unsafe(no_mangle)]
pub extern "C" fn srand(seed: u32) {
    raven_libc_stdlib::rand::srand(crate::stdlib::rand_state::default_generator(), seed);
}
