// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the pidfile export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `pidfile` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_compat::PidfileOps;
use raven_libc_compat::pidfile::{PidState, cleanup, pidfile as logic};

/// The real process operations.
struct RealPid;

fn cleanup_hook() {
    // SAFETY: the C API is not re-entrant; the static state is the donor's global.
    unsafe { cleanup(&mut RealPid, &*(&raw const STATE)) };
}

static mut STATE: PidState = PidState {
    path: alloc::vec::Vec::new(),
    pid: -1,
    atexit_hook: cleanup_hook,
};

impl PidfileOps for RealPid {
    fn create_write(
        &mut self,
        path: &[u8],
        contents: &[u8],
    ) -> Result<(), raven_libc_abi::errno::Errno> {
        if path.len() >= super::MAX_NAME {
            return Err(raven_libc_abi::errno::Errno(super::ENAMETOOLONG as u16));
        }
        let mut name = [0i8; 4096];
        for (slot, byte) in name.iter_mut().zip(path.iter()) {
            *slot = *byte as i8;
        }
        name[path.len()] = 0;
        // O_WRONLY|O_CREAT|O_EXCL|O_TRUNC, mode 0644.
        // SAFETY: `name` is NUL-terminated.
        let descriptor = unsafe { super::open(name.as_ptr(), 1 | 0o100 | 0o200 | 0o1000, 0o644) };
        if descriptor < 0 {
            return Err(raven_libc_abi::errno::Errno(super::EINVAL as u16));
        }
        // SAFETY: the descriptor is open and `contents` is readable.
        let written = unsafe { super::write(descriptor, contents.as_ptr().cast(), contents.len()) };
        // SAFETY: the descriptor is open.
        unsafe { super::close(descriptor) };
        if written != isize::try_from(contents.len()).unwrap_or(-1) {
            return Err(raven_libc_abi::errno::Errno(super::EINVAL as u16));
        }
        Ok(())
    }

    fn unlink(&mut self, path: &[u8]) {
        let mut name = [0i8; 4096];
        if path.len() >= name.len() {
            return;
        }
        for (slot, byte) in name.iter_mut().zip(path.iter()) {
            *slot = *byte as i8;
        }
        name[path.len()] = 0;
        // SAFETY: `name` is NUL-terminated.
        unsafe { super::unlink(name.as_ptr()) };
    }

    fn getpid(&self) -> i32 {
        // SAFETY: `getpid` takes no arguments.
        unsafe { super::getpid() }
    }

    fn atexit(&mut self, hook: fn()) -> Result<(), raven_libc_abi::errno::Errno> {
        extern "C" fn trampoline() {
            // SAFETY: the static holds the hook the caller registered.
            let hook = unsafe { STATE.atexit_hook };
            hook();
        }
        let _ = hook;
        // SAFETY: `atexit` accepts a C function pointer.
        if unsafe { super::atexit(trampoline) } == 0 {
            Ok(())
        } else {
            Err(raven_libc_abi::errno::Errno(super::ENOMEM as u16))
        }
    }
}

/// `pidfile`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pidfile(basename: *const c_char) -> c_int {
    let name = if basename.is_null() {
        None
    } else {
        // SAFETY: `basename` is NUL-terminated.
        Some(unsafe { super::c_bytes(basename, super::MAX_NAME) })
    };
    // SAFETY: the static state is the donor's global.
    match logic(&mut RealPid, name, b"", unsafe { &mut *(&raw mut STATE) }) {
        Ok(()) => 0,
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            -1
        }
    }
}
