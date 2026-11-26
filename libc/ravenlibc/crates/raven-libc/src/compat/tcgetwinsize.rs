// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tcgetwinsize`: the pty stores the window size; a non-tty fd answers `ENOTTY`.

use core::ffi::c_int;

use raven_libc_compat::Winsize;

use super::fail;
use super::tty_ipc_state::map;

/// `tcgetwinsize`.
///
/// # Safety
/// `winsize` points to a writable `struct winsize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcgetwinsize(fd: c_int, winsize: *mut Winsize) -> c_int {
    if winsize.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    match raven_libc_platform::ioctl::tty::tcgetwinsize(crate::ffi::platform::global(), fd) {
        Ok(size) => {
            // SAFETY: the caller contract above grants a writable `struct winsize`.
            unsafe {
                core::ptr::write(
                    winsize,
                    Winsize {
                        ws_row: size.row,
                        ws_col: size.col,
                        ws_xpixel: size.xpixel,
                        ws_ypixel: size.ypixel,
                    },
                );
            }
            0
        }
        Err(error) => map(error),
    }
}
