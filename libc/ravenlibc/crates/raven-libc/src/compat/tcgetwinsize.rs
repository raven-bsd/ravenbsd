// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tcgetwinsize`: the tty protocol has no window-size operation yet, so the answer is `ENOSYS`.

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
        Ok(()) => {
            // SAFETY: the caller contract above grants a writable `struct winsize`.
            unsafe { core::ptr::write(winsize, Winsize::default()) };
            0
        }
        Err(error) => map(error),
    }
}
