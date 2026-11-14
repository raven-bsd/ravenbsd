// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `srand48(3)`.
use core::ffi::c_long;

/// Seed the process 48-bit generator.
#[unsafe(no_mangle)]
pub extern "C" fn srand48(seed: c_long) {
    raven_libc_stdlib::rand::srand48(crate::stdlib::rand_state::generator48(), seed as i32);
}
