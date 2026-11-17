// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tcflush`: validate the queue selector and use the tty scheme's `Flush` request.

use core::ffi::c_int;

use raven_libc_ipc::termios;

use super::fail;
use super::tty_ipc_state::map;

/// `tcflush`.
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcflush(fd: c_int, queue: c_int) -> c_int {
    if let Err(error) = termios::validate_queue(queue) {
        return fail(error);
    }
    match raven_libc_platform::ioctl::tty::tcflush(crate::ffi::platform::global(), fd, queue) {
        Ok(()) => 0,
        Err(error) => map(error),
    }
}
