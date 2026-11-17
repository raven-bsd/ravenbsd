// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tcsetwinsize`: the tty protocol has no window-size operation yet, so the answer is `ENOSYS`.

use core::ffi::c_int;

use raven_libc_compat::Winsize;

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
    match raven_libc_platform::ioctl::tty::tcsetwinsize(crate::ffi::platform::global(), fd) {
        Ok(()) => 0,
        Err(error) => map(error),
    }
}
