// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_yield(2)`.
use core::ffi::c_int;

/// Give up the CPU with the `YIELD` system call.
#[unsafe(no_mangle)]
pub extern "C" fn sched_yield() -> c_int {
    let result = raven_libc_sys::sys::yield_now();
    crate::ffi::ret::int(
        result
            .map(|()| 0)
            .map_err(|error| raven_libc_abi::errno::Errno(error.0)),
    )
}
