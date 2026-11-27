// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `ioctl` trampoline target: classify the request before reading its argument, then
//! dispatch to the typed terminal, descriptor or kernel-session operation.

use core::ffi::{c_int, c_ulong};

use raven_libc_abi::structs::Termios;
use raven_libc_compat::Winsize;
use raven_libc_platform::Errno;
use raven_libc_platform::xlat::{
    FIOCLEX, FIONBIO, FIONCLEX, FIONREAD, TIOCGETA, TIOCGPGRP, TIOCGWINSZ, TIOCSCTTY, TIOCSETA,
    TIOCSETAF, TIOCSETAW, TIOCSPGRP, TIOCSWINSZ,
};

use crate::compat::pty_state::PROC_CTTY_SET;
use crate::stdio::va::{List, reader};
use crate::va::VaReaderExt;

/// What the third `ioctl` argument is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Argument {
    /// The request takes no third argument.
    None,
    /// The request takes a pointer to the typed object or integer.
    Pointer,
    /// A declared request that this provider does not implement; do not dereference its pointer.
    UnsupportedPointer,
}

/// Classify the third argument from the request before the variadic list is touched.
fn argument(request: u32) -> Option<Argument> {
    match request {
        TIOCSCTTY | FIOCLEX | FIONCLEX => Some(Argument::None),
        TIOCGETA | TIOCSETA | TIOCSETAW | TIOCSETAF | TIOCGWINSZ | TIOCSWINSZ | TIOCGPGRP
        | TIOCSPGRP | FIONBIO => Some(Argument::Pointer),
        FIONREAD => Some(Argument::UnsupportedPointer),
        _ => None,
    }
}

/// `ioctl(fd, request, ...)`: the request decides whether an argument is read at all.
///
/// # Safety
/// The C59 trampoline built `list`; pointer requests need the caller's matching object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_ioctl(fd: c_int, request: c_int, list: *mut List) -> c_int {
    let request = request as c_ulong;
    let Some(kind) = argument(request as u32) else {
        return crate::ffi::ret::platform_int(Err(Errno::ENOTTY));
    };
    if kind == Argument::UnsupportedPointer {
        return crate::ffi::ret::platform_int(Err(Errno::ENOTTY));
    }
    // SAFETY: the trampoline's list is valid for the call.
    let mut values = unsafe { reader(list) };
    let argument = match kind {
        Argument::None => 0,
        Argument::Pointer => values.next_ptr(),
        Argument::UnsupportedPointer => 0,
    };
    let result = dispatch(fd, request as u32, argument);
    crate::ffi::ret::platform_int(result.map(|value| value as c_int))
}

/// Route one classified request to the scalar, typed or kernel-session entry.
fn dispatch(fd: c_int, request: u32, argument: usize) -> Result<i32, Errno> {
    match request {
        TIOCGETA => {
            if argument == 0 {
                return Err(Errno::EFAULT);
            }
            let value =
                raven_libc_platform::ioctl::ioctl_tcgets(crate::ffi::platform::global(), fd)?;
            // SAFETY: the caller's TIOCGETA contract is a writable `struct termios`.
            unsafe { core::ptr::write(argument as *mut Termios, value) };
            Ok(0)
        }
        TIOCSETA | TIOCSETAW | TIOCSETAF => set_termios(fd, argument),
        TIOCGWINSZ => {
            if argument == 0 {
                return Err(Errno::EFAULT);
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
                return Err(Errno::EFAULT);
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
        TIOCGPGRP => {
            if argument == 0 {
                return Err(Errno::EFAULT);
            }
            let pgrp = crate::platform::tcgetpgrp::get_pgrp(fd)?;
            // SAFETY: the caller's TIOCGPGRP contract is a writable `int`.
            unsafe { core::ptr::write(argument as *mut i32, pgrp) };
            Ok(0)
        }
        TIOCSPGRP => {
            if argument == 0 {
                return Err(Errno::EFAULT);
            }
            // SAFETY: the caller's TIOCSPGRP contract is a readable `int`.
            let pgrp = unsafe { core::ptr::read(argument as *const i32) };
            crate::platform::tcsetpgrp::set_pgrp(fd, pgrp)?;
            Ok(0)
        }
        // `TIOCSCTTY` is a kernel session operation, not a scheme CALL: give the kernel the
        // descriptor's live terminal handle.
        TIOCSCTTY => {
            let handle = crate::ffi::platform::global().get(fd)?.handle();
            // SAFETY: `PROC_CTTY_SET` takes one live scheme-file handle owned by the caller.
            let raw = unsafe { raven_libc_sys::arch::syscall1(PROC_CTTY_SET, handle.0 as usize) };
            raven_libc_sys::errno::ret(raw)?;
            Ok(0)
        }
        // FreeBSD FIONBIO takes a pointer to an `int`, not the scalar value.
        FIONBIO => {
            if argument == 0 {
                return Err(Errno::EFAULT);
            }
            // SAFETY: the caller's FIONBIO contract is a readable `int`.
            let value = unsafe { core::ptr::read(argument as *const i32) };
            raven_libc_platform::ioctl::ioctl(crate::ffi::platform::global(), fd, request, value)
        }
        FIOCLEX | FIONCLEX => {
            raven_libc_platform::ioctl::ioctl(crate::ffi::platform::global(), fd, request, 0)
        }
        _ => Err(Errno::ENOTTY),
    }
}

/// Copy the caller's `struct termios` through the typed platform setter.
fn set_termios(fd: c_int, argument: usize) -> Result<i32, Errno> {
    if argument == 0 {
        return Err(Errno::EFAULT);
    }
    // SAFETY: the caller's TIOCSETA contract is a readable `struct termios`.
    let value = unsafe { core::ptr::read(argument as *const Termios) };
    raven_libc_platform::ioctl::ioctl_tcsets(crate::ffi::platform::global(), fd, &value)?;
    Ok(0)
}
