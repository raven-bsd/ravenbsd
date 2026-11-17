// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tcdrain`: the tty protocol has no output-drain operation yet, so the answer is `ENOSYS`.

use core::ffi::c_int;

use super::tty_ipc_state::map;

/// `tcdrain`.
///
/// # Safety
/// Scalar argument only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcdrain(fd: c_int) -> c_int {
    match raven_libc_platform::ioctl::tty::tcdrain(crate::ffi::platform::global(), fd) {
        Ok(()) => 0,
        Err(error) => map(error),
    }
}
