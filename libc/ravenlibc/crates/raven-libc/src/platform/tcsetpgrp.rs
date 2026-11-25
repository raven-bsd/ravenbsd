// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tcsetpgrp(3)`.
//!
//! The kernel validates the controlling terminal, the caller's session and the target process
//! group (`PROC_CTTY_SET_FG`); libc does not cache or pre-empt that decision.

use core::ffi::c_int;

use raven_libc_platform::Errno;

/// Raw `tcsetpgrp(handle, pgrp) -> 0` (`kernel/src/syscall/tty.rs`).
pub(crate) const PROC_CTTY_SET_FG: usize = 0x1e2;

/// Set the foreground process group of `fd` without touching `errno`.
pub(crate) fn set_pgrp(fd: c_int, pgrp: c_int) -> Result<(), Errno> {
    if pgrp < 0 {
        return Err(Errno(raven_libc_abi::errno::EINVAL as u16));
    }
    let handle = crate::ffi::platform::global().get(fd)?.handle();
    // SAFETY: both arguments are scalar; the kernel validates the handle and process group.
    let raw = unsafe {
        raven_libc_sys::arch::syscall2(PROC_CTTY_SET_FG, handle.0 as usize, pgrp as isize as usize)
    };
    raven_libc_sys::errno::ret(raw).map(|_| ())
}

/// Set the foreground process group of `fd`, or `-1` with the POSIX errno.
#[unsafe(no_mangle)]
pub extern "C" fn tcsetpgrp(fd: c_int, pgrp: c_int) -> c_int {
    match set_pgrp(fd, pgrp) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
