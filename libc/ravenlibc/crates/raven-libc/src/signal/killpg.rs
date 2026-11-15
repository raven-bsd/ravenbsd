// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `killpg(3)`.

use core::ffi::c_int;

use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::send;

/// Send `sig` to the process group `pgrp`.
#[unsafe(no_mangle)]
pub extern "C" fn killpg(pgrp: c_int, sig: c_int) -> c_int {
    match send::killpg(&SysKernel, pgrp, sig) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
