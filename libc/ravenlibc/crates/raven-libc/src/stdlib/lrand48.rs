// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `lrand48(3)`.
use core::ffi::c_long;

/// A non-negative `long` from the process 48-bit generator.
#[unsafe(no_mangle)]
pub extern "C" fn lrand48() -> c_long {
    raven_libc_stdlib::rand::lrand48(crate::stdlib::rand_state::generator48())
}
