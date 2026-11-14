// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_getcpu(3)`.
use core::ffi::c_int;

/// The single-CPU system runs on CPU 0 (O-15-2).
#[unsafe(no_mangle)]
pub extern "C" fn sched_getcpu() -> c_int {
    0
}
