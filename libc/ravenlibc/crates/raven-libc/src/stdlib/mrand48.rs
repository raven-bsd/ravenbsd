// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mrand48(3)`.
use core::ffi::c_long;

/// A signed `long` from the process 48-bit generator.
#[unsafe(no_mangle)]
pub extern "C" fn mrand48() -> c_long {
    raven_libc_stdlib::rand::mrand48(crate::stdlib::rand_state::generator48())
}
