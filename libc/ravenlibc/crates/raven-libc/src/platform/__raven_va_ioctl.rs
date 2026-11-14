// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `ioctl` trampoline target: one integer/pointer third argument, then C55's dispatch.
use core::ffi::{c_int, c_ulong};

use raven_libc_abi::structs::Termios;
use raven_libc_platform::xlat::{FIOCLEX, FIONBIO, TCGETS, TCSETS, TCSETSF, TCSETSW};

use crate::stdio::va::{List, reader};
use crate::va::VaReaderExt;

/// `ioctl(fd, request, ...)`: scalar requests go to C55, termios requests use the typed calls.
///
/// # Safety
/// The C59 trampoline built `list`; pointer requests need the caller's matching object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_ioctl(fd: c_int, request: c_int, list: *mut List) -> c_int {
    // SAFETY: the trampoline's list is valid for the call.
    let mut values = unsafe { reader(list) };
    let argument = values.next_ptr() as usize;
    let result = dispatch(fd, request as c_ulong, argument);
    crate::ffi::ret::platform_int(result.map(|value| value as c_int))
}

/// Route one request to the platform's scalar or typed entry.
fn dispatch(
    fd: c_int,
    request: c_ulong,
    argument: usize,
) -> Result<i32, raven_libc_platform::Errno> {
    let request = request as u32;
    match request {
        TCGETS => {
            if argument == 0 {
                return Err(raven_libc_platform::Errno::EFAULT);
            }
            let value =
                raven_libc_platform::ioctl::ioctl_tcgets(crate::ffi::platform::global(), fd)?;
            // SAFETY: the caller's TCGETS contract is a writable `struct termios`.
            unsafe { core::ptr::write(argument as *mut Termios, value) };
            Ok(0)
        }
        TCSETS | TCSETSW | TCSETSF => {
            if argument == 0 {
                return Err(raven_libc_platform::Errno::EFAULT);
            }
            // SAFETY: the caller's TCSETS contract is a readable `struct termios`.
            let value = unsafe { core::ptr::read(argument as *const Termios) };
            raven_libc_platform::ioctl::ioctl_tcsets(crate::ffi::platform::global(), fd, &value)?;
            Ok(0)
        }
        FIONBIO | FIOCLEX => raven_libc_platform::ioctl::ioctl(
            crate::ffi::platform::global(),
            fd,
            request,
            argument as i32,
        ),
        _ => raven_libc_platform::ioctl::ioctl(
            crate::ffi::platform::global(),
            fd,
            request,
            argument as i32,
        ),
    }
}
