// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tcsetpgrp(3)`.
//!
//! The tty scheme does not expose the foreground process group yet (PG-54); the symbol reports
//! `ENOSYS` rather than accepting a group it cannot deliver.

use core::ffi::c_int;

/// Set the foreground process group of `fd`, or `-1` with `ENOSYS`.
#[unsafe(no_mangle)]
pub extern "C" fn tcsetpgrp(_fd: c_int, _pgrp: c_int) -> c_int {
    crate::ffi::errno::set(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    ));
    -1
}
