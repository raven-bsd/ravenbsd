// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `ioctl` trampoline target: one integer/pointer third argument, then C55's dispatch.
use core::ffi::{c_int, c_ulong};

use raven_libc_abi::structs::Termios;
use raven_libc_compat::Winsize;
use raven_libc_platform::xlat::{
    FIOCLEX, FIONBIO, TCGETS, TCSETS, TCSETSF, TCSETSW, TIOCGWINSZ, TIOCSWINSZ,
};

use crate::compat::TIOCSCTTY;
use crate::compat::pty_state::PROC_CTTY_SET;
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
        TIOCGWINSZ => {
            if argument == 0 {
                return Err(raven_libc_platform::Errno::EFAULT);
            }
            let size =
                raven_libc_platform::ioctl::tcgetwinsize(crate::ffi::platform::global(), fd)?;
            // SAFETY: the caller's TIOCGWINSZ contract is a writable `struct winsize`.
            unsafe {
                core::ptr::write(
                    argument as *mut Winsize,
                    Winsize {
                        ws_row: size.row,
                        ws_col: size.col,
                        ws_xpixel: size.xpixel,
                        ws_ypixel: size.ypixel,
                    },
                );
            }
            Ok(0)
        }
        TIOCSWINSZ => {
            if argument == 0 {
                return Err(raven_libc_platform::Errno::EFAULT);
            }
            // SAFETY: the caller's TIOCSWINSZ contract is a readable `struct winsize`.
            let value = unsafe { core::ptr::read(argument as *const Winsize) };
            let size = raven_libc_platform::ioctl::Winsize {
                row: value.ws_row,
                col: value.ws_col,
                xpixel: value.ws_xpixel,
                ypixel: value.ws_ypixel,
            };
            raven_libc_platform::ioctl::tcsetwinsize(crate::ffi::platform::global(), fd, &size)?;
            Ok(0)
        }
        FIONBIO | FIOCLEX => raven_libc_platform::ioctl::ioctl(
            crate::ffi::platform::global(),
            fd,
            request,
            argument as i32,
        ),
        // `TIOCSCTTY` is a kernel session operation, not a scheme CALL: give the kernel the
        // descriptor's live terminal handle. OpenSSH `pty_make_controlling_tty` calls this.
        request if request == TIOCSCTTY as u32 => {
            let handle = crate::ffi::platform::global().get(fd)?.handle();
            // SAFETY: `PROC_CTTY_SET` takes one live scheme-file handle owned by the caller.
            let raw = unsafe { raven_libc_sys::arch::syscall1(PROC_CTTY_SET, handle.0 as usize) };
            raven_libc_sys::errno::ret(raw)?;
            Ok(0)
        }
        _ => raven_libc_platform::ioctl::ioctl(
            crate::ffi::platform::global(),
            fd,
            request,
            argument as i32,
        ),
    }
}
