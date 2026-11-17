// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tcflow`: validate the action and report `ENOSYS` until the tty protocol carries flow control.

use core::ffi::c_int;

use raven_libc_ipc::termios;

use super::fail;
use super::tty_ipc_state::map;

/// `tcflow`.
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcflow(fd: c_int, action: c_int) -> c_int {
    if let Err(error) = termios::validate_flow(action) {
        return fail(error);
    }
    match raven_libc_platform::ioctl::tty::tcflow(crate::ffi::platform::global(), fd, action) {
        Ok(()) => 0,
        Err(error) => map(error),
    }
}
