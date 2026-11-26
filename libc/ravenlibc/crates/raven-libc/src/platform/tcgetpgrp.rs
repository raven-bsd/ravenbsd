// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tcgetpgrp(3)`.
//!
//! The kernel owns the controlling-terminal foreground group (`PROC_CTTY_GET`); libc only
//! presents the C return/errno contract.

use core::ffi::c_int;

use raven_libc_platform::Errno;

/// Raw `tcgetpgrp(handle, out: *mut i32) -> 0` (`kernel/src/syscall/tty.rs`).
pub(crate) const PROC_CTTY_GET: usize = 0x1e1;

/// Return the foreground process group of `fd` without touching `errno`.
pub(crate) fn get_pgrp(fd: c_int) -> Result<c_int, Errno> {
    let handle = crate::ffi::platform::global().get(fd)?.handle();
    let mut pgrp = 0i32;
    // SAFETY: `pgrp` is a live i32 and the kernel writes exactly four bytes through the pointer.
    let raw = unsafe {
        raven_libc_sys::arch::syscall2(
            PROC_CTTY_GET,
            handle.0 as usize,
            &mut pgrp as *mut i32 as usize,
        )
    };
    raven_libc_sys::errno::ret(raw)?;
    Ok(pgrp)
}

/// Return the foreground process group of `fd`, or `-1` with the platform errno.
#[unsafe(no_mangle)]
pub extern "C" fn tcgetpgrp(fd: c_int) -> c_int {
    match get_pgrp(fd) {
        Ok(pgrp) => pgrp,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
