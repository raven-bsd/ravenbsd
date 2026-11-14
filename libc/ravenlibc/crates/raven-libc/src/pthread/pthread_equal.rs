// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_equal(3)`.
use raven_libc_thread::thread::join;

/// Compare two thread handles.
#[unsafe(no_mangle)]
pub extern "C" fn pthread_equal(a: usize, b: usize) -> i32 {
    i32::from(join::equal(a, b))
}
