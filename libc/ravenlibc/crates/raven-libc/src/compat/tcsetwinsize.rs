// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tcsetwinsize`: the pty stores the window size; a non-tty fd answers `ENOTTY`.

use core::ffi::c_int;

use raven_libc_compat::Winsize;
use raven_libc_platform::ioctl::tty::Winsize as PlatformWinsize;

use super::fail;
use super::tty_ipc_state::map;

/// `tcsetwinsize`.
///
/// # Safety
/// `winsize` points to a readable `struct winsize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcsetwinsize(fd: c_int, winsize: *const Winsize) -> c_int {
    if winsize.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    // SAFETY: the caller contract above grants a readable `struct winsize`.
    let value = unsafe { core::ptr::read(winsize) };
    let size = PlatformWinsize {
        row: value.ws_row,
        col: value.ws_col,
        xpixel: value.ws_xpixel,
        ypixel: value.ws_ypixel,
    };
    match raven_libc_platform::ioctl::tty::tcsetwinsize(crate::ffi::platform::global(), fd, &size) {
        Ok(()) => 0,
        Err(error) => map(error),
    }
}
