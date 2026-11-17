// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `fcntl` trampoline target: read the command's optional argument and forward.

use core::ffi::c_int;

use raven_libc_abi::fcntl::{F_GETLK, F_SETLK, F_SETLKW};
use raven_libc_platform::Errno;

use crate::stdio::va::{List, reader};
use crate::va::VaReaderExt;

use super::fcntl_args::{self, Argument};

/// `fcntl(fd, command, ...)` with the third argument read only when the command has one.
///
/// # Safety
/// The C59 trampoline built `list`; pointer commands carry the caller's matching object.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_fcntl(fd: c_int, command: c_int, list: *mut List) -> c_int {
    // SAFETY: the trampoline's list is valid for the call.
    let mut values = unsafe { reader(list) };
    let argument = match fcntl_args::argument(command) {
        Argument::Scalar => i64::from(values.next_i32()),
        Argument::Pointer => values.next_ptr() as i64,
        Argument::None => 0,
    };
    let result = match command {
        F_GETLK | F_SETLK | F_SETLKW => crate::fcntl::record_lock(fd, command, argument as usize),
        _ => crate::ffi::platform::global()
            .fcntl(fd, command, argument)
            .and_then(|value| i32::try_from(value).map_err(|_| Errno::EOVERFLOW)),
    };
    crate::ffi::ret::platform_int(result)
}
