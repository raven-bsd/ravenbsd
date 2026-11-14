// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigignore(3)`.
use super::DISP;
use core::ffi::c_int;
use raven_libc_signal::legacy;
/// Ignore `sig`.
#[unsafe(no_mangle)]
pub extern "C" fn sigignore(sig: c_int) -> c_int {
    match legacy::sigignore(&DISP, sig) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
