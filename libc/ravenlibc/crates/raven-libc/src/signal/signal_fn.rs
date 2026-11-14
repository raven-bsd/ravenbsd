// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `signal(3)`.
use core::ffi::c_int;

use raven_libc_signal::legacy;

use super::DISP;

/// Install a handler with BSD reliable semantics.
#[unsafe(no_mangle)]
pub extern "C" fn signal(sig: c_int, handler: usize) -> usize {
    match legacy::signal(&DISP, sig, handler) {
        Ok(old) => old,
        Err(error) => {
            crate::ffi::errno::set(error);
            usize::MAX
        }
    }
}
