// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `drand48(3)`.
/// A double in `[0.0, 1.0)` from the process 48-bit generator.
#[unsafe(no_mangle)]
pub extern "C" fn drand48() -> f64 {
    raven_libc_stdlib::rand::drand48(crate::stdlib::rand_state::generator48())
}
