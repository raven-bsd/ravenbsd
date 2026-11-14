// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `siginterrupt(3)`.
use super::DISP;
use core::ffi::c_int;
use raven_libc_signal::legacy;
/// Set the `SA_RESTART` policy for `sig`.
#[unsafe(no_mangle)]
pub extern "C" fn siginterrupt(sig: c_int, flag: c_int) -> c_int {
    match legacy::siginterrupt(&DISP, sig, flag) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
