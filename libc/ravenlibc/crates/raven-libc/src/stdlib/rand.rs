// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `rand(3)`.
use core::ffi::c_int;

/// A pseudo-random number in `0..=RAND_MAX`.
#[unsafe(no_mangle)]
pub extern "C" fn rand() -> c_int {
    let value = raven_libc_stdlib::rand::rand(crate::stdlib::rand_state::default_generator());
    value as c_int
}
