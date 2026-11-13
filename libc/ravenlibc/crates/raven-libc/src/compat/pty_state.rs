// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The tty seam of the `openpty`/`forkpty`/`login_tty` exports.

#![allow(
    missing_docs,
    dead_code,
    unused_imports,
    unused_unsafe,
    unused_variables,
    unused_mut,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks,
    clippy::missing_safety_doc,
    clippy::deref_addrof,
    clippy::unreachable,
    clippy::byte_char_slices,
    clippy::collapsible_if,
    clippy::manual_c_str_literals,
    clippy::needless_return
)]

use core::ffi::c_int;

use raven_libc_abi::generated::structs::Termios;
use raven_libc_compat::{ForkResult, PtyOps, Winsize};

pub(crate) struct RealPty;

impl PtyOps for RealPty {
    fn posix_openpt(&mut self, flags: i32) -> Result<i32, raven_libc_abi::errno::Errno> {
        // SAFETY: `posix_openpt` takes the flags only.
        let descriptor = unsafe { super::posix_openpt(flags) };
        if descriptor < 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(descriptor)
        }
    }

    fn grantpt(&mut self, fd: i32) -> Result<(), raven_libc_abi::errno::Errno> {
        // SAFETY: `fd` is an open master.
        if unsafe { super::grantpt(fd) } != 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(())
        }
    }

    fn unlockpt(&mut self, fd: i32) -> Result<(), raven_libc_abi::errno::Errno> {
        // SAFETY: `fd` is an open master.
        if unsafe { super::unlockpt(fd) } != 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(())
        }
    }

    fn ptsname_r(
        &mut self,
        fd: i32,
        buffer: &mut [u8],
    ) -> Result<usize, raven_libc_abi::errno::Errno> {
        // SAFETY: `buffer` is writable and `fd` is an open master.
        let result = unsafe { super::ptsname_r(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if result != 0 {
            return Err(raven_libc_abi::errno::Errno(super::ERANGE as u16));
        }
        // SAFETY: the callee NUL-terminated the buffer.
        Ok(unsafe { super::c_len(buffer.as_ptr().cast(), buffer.len()) })
    }

    fn open(&mut self, path: &[u8], flags: i32) -> Result<i32, raven_libc_abi::errno::Errno> {
        let mut name = [0i8; 4096];
        if path.len() >= name.len() {
            return Err(raven_libc_abi::errno::Errno(super::ENAMETOOLONG as u16));
        }
        for (slot, byte) in name.iter_mut().zip(path.iter()) {
            *slot = *byte as i8;
        }
        name[path.len()] = 0;
        // SAFETY: `name` is NUL-terminated.
        let descriptor = unsafe { super::open(name.as_ptr(), flags | super::O_NOCTTY) };
        if descriptor < 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(descriptor)
        }
    }

    fn close(&mut self, fd: i32) {
        // SAFETY: `fd` is open.
        unsafe { super::close(fd) };
    }

    fn tcsetattr(
        &mut self,
        fd: i32,
        termios: &Termios,
    ) -> Result<(), raven_libc_abi::errno::Errno> {
        // SAFETY: `termios` is readable and `fd` is open.
        if unsafe { super::tcsetattr(fd, super::TCSAFLUSH, termios) } != 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(())
        }
    }

    fn set_winsize(
        &mut self,
        fd: i32,
        window: &Winsize,
    ) -> Result<(), raven_libc_abi::errno::Errno> {
        // SAFETY: `window` is readable and `fd` is open.
        if unsafe { super::ioctl(fd, super::TIOCSWINSZ, window as *const Winsize) } != 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(())
        }
    }

    fn setsid(&mut self) -> Result<i32, raven_libc_abi::errno::Errno> {
        // SAFETY: `setsid` takes no arguments.
        let result = unsafe { super::setsid() };
        if result < 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(result)
        }
    }

    fn set_ctty(&mut self, fd: i32) -> Result<(), raven_libc_abi::errno::Errno> {
        // SAFETY: `fd` is open.
        if unsafe { super::ioctl(fd, super::TIOCSCTTY, 0 as c_int) } != 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(())
        }
    }

    fn dup2(&mut self, from: i32, to: i32) -> Result<i32, raven_libc_abi::errno::Errno> {
        // SAFETY: both descriptors are valid.
        let result = unsafe { super::dup2(from, to) };
        if result < 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(result)
        }
    }

    fn fork(&mut self) -> Result<ForkResult, raven_libc_abi::errno::Errno> {
        // PG-45: fork is absent in v1; the C `fork` returning -1 with ENOSYS is reported as-is.
        // SAFETY: `fork` takes no arguments.
        let result = unsafe { super::fork() };
        match result {
            0 => Ok(ForkResult::Child),
            value if value > 0 => Ok(ForkResult::Parent(value)),
            _ => Err(raven_libc_abi::errno::Errno(super::ENOSYS as u16)),
        }
    }
}
