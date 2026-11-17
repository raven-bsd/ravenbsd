// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tcsendbreak`: the tty protocol has no break operation yet, so the answer is `ENOSYS`.

use core::ffi::c_int;

use super::tty_ipc_state::map;

/// `tcsendbreak`.
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcsendbreak(fd: c_int, duration: c_int) -> c_int {
    match raven_libc_platform::ioctl::tty::tcsendbreak(crate::ffi::platform::global(), fd, duration)
    {
        Ok(()) => 0,
        Err(error) => map(error),
    }
}
