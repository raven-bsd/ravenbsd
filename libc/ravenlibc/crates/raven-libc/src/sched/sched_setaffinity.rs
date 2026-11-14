// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_setaffinity(2)`.
use core::ffi::c_int;

/// Set the CPU affinity: `ENOSYS` until the primitive exists (O-15-8).
///
/// # Safety
/// `mask` follows the C contract and is not read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sched_setaffinity(pid: c_int, size: usize, mask: *const u8) -> c_int {
    let _ = (pid, size, mask);
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}
