// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the readpassphrase export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `readpassphrase` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_abi::generated::structs::Termios;
use raven_libc_compat::TtyOps;
use raven_libc_compat::readpassphrase as logic;

static mut SIGNAL_NUMBER: c_int = 0;
static mut OLD_HANDLERS: [usize; 32] = [0; 32];

extern "C" fn handler(signum: c_int) {
    // SAFETY: the handler only stores one word in a static (async-signal-safe).
    unsafe { *&raw mut SIGNAL_NUMBER = signum };
}

const SIGNALS: [c_int; 8] = [1, 2, 3, 13, 14, 15, 20, 21];

struct RealTty;

impl TtyOps for RealTty {
    fn open_tty(&mut self) -> Result<i32, raven_libc_abi::errno::Errno> {
        let path = b"/dev/tty\0";
        // SAFETY: `path` is NUL-terminated; O_RDWR|O_NOCTTY.
        let descriptor =
            unsafe { super::open(path.as_ptr().cast(), super::O_RDWR | super::O_NOCTTY) };
        if descriptor < 0 {
            Err(raven_libc_abi::errno::Errno(super::ENOTTY as u16))
        } else {
            Ok(descriptor)
        }
    }

    fn tcgetattr(&mut self, fd: i32) -> Result<Termios, raven_libc_abi::errno::Errno> {
        let mut termios = Termios::ZERO;
        // SAFETY: `termios` is writable and `fd` is open.
        if unsafe { super::tcgetattr(fd, &mut termios) } != 0 {
            return Err(raven_libc_abi::errno::Errno(super::ENOTTY as u16));
        }
        Ok(termios)
    }

    fn tcsetattr(
        &mut self,
        fd: i32,
        termios: &Termios,
    ) -> Result<(), raven_libc_abi::errno::Errno> {
        // SAFETY: `termios` is readable and `fd` is open.
        if unsafe { super::tcsetattr(fd, super::TCSAFLUSH, termios) } != 0 {
            return Err(raven_libc_abi::errno::Errno(super::EINVAL as u16));
        }
        Ok(())
    }

    fn write(&mut self, fd: i32, bytes: &[u8]) -> Result<usize, raven_libc_abi::errno::Errno> {
        // SAFETY: `bytes` is readable and `fd` is open.
        let count = unsafe { super::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if count < 0 {
            Err(raven_libc_abi::errno::Errno(super::EINVAL as u16))
        } else {
            Ok(usize::try_from(count).unwrap_or(0))
        }
    }

    fn read_byte(&mut self, fd: i32) -> Result<Option<u8>, raven_libc_abi::errno::Errno> {
        let mut byte = 0u8;
        // SAFETY: `byte` is writable and `fd` is open.
        let count = unsafe { super::read(fd, (&mut byte as *mut u8).cast(), 1) };
        match count {
            0 => Ok(None),
            value if value > 0 => Ok(Some(byte)),
            _ => Err(raven_libc_abi::errno::Errno(4)),
        }
    }

    fn close(&mut self, fd: i32) {
        // SAFETY: `fd` is open.
        unsafe { super::close(fd) };
    }

    fn pending_signal(&mut self) -> Option<i32> {
        // SAFETY: the handler wrote this static; the shell is not re-entrant.
        let signal = unsafe { *&raw mut SIGNAL_NUMBER };
        if signal == 0 {
            return None;
        }
        // SAFETY: clear the flag.
        unsafe { *&raw mut SIGNAL_NUMBER = 0 };
        Some(signal)
    }
}

fn install() {
    for signum in SIGNALS {
        let index = usize::try_from(signum).unwrap_or(0);
        // SAFETY: `signal` installs the handler and returns the old one.
        let pointer: extern "C" fn(c_int) = handler;
        let previous = unsafe { super::signal(signum, pointer as usize) };
        if index < 32 {
            // SAFETY: the static is written only here.
            unsafe { (*&raw mut OLD_HANDLERS)[index] = previous };
        }
    }
}

fn restore() {
    for signum in SIGNALS {
        let index = usize::try_from(signum).unwrap_or(0);
        if index < 32 {
            // SAFETY: the static was filled by `install`.
            let previous = unsafe { (*&raw const OLD_HANDLERS)[index] };
            // SAFETY: restore the previous handler.
            unsafe { super::signal(signum, previous) };
        }
    }
}

/// `readpassphrase`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readpassphrase(
    prompt: *const c_char,
    buffer: *mut c_char,
    bufsiz: usize,
    flags: c_int,
) -> *mut c_char {
    if buffer.is_null() || bufsiz == 0 {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return core::ptr::null_mut();
    }
    // SAFETY: `prompt` is NUL-terminated or null.
    let prompt = unsafe { super::c_bytes(prompt, super::MAX_CSTR) };
    install();
    // SAFETY: the caller gives `bufsiz` writable bytes.
    let destination = unsafe { super::out_bytes(buffer, bufsiz) };
    let result = logic::readpassphrase(&mut RealTty, prompt, destination, flags);
    restore();
    match result {
        Some(_) => buffer,
        None => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(super::ENOTTY) };
            core::ptr::null_mut()
        }
    }
}
