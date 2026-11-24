// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tcsetpgrp(3)`.
//!
//! POSIX checks the group against the caller's session before the pty server stores it; the
//! server exposes the `SET_FG` custom CALL op.

use core::ffi::c_int;

use raven_libc_abi::errno::{EINVAL, EPERM};
use raven_libc_proc::backend::ProcBackend;

fn fail(number: i32) -> c_int {
    crate::ffi::errno::set(raven_libc_abi::errno::Errno(number as u16));
    -1
}

/// Set the foreground process group of `fd`, or `-1` with the POSIX errno.
#[unsafe(no_mangle)]
pub extern "C" fn tcsetpgrp(fd: c_int, pgrp: c_int) -> c_int {
    if pgrp <= 0 {
        return fail(EINVAL);
    }
    let backend = crate::ffi::procbackend::backend();
    let Ok(own_session) = backend.getsid(0) else {
        return fail(EINVAL);
    };
    let Ok(group_session) = backend.getsid(pgrp) else {
        return fail(EINVAL);
    };
    if own_session != group_session {
        return fail(EPERM);
    }
    match raven_libc_platform::ioctl::tty::tcsetpgrp(crate::ffi::platform::global(), fd, pgrp) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
