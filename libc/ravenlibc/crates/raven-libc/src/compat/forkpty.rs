// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the forkpty export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `forkpty` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};
use raven_libc_abi::generated::structs::Termios;
use raven_libc_compat::{ForkResult, Winsize};

use super::pty_state::RealPty;

/// `forkpty`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn forkpty(
    master: *mut c_int,
    name: *mut c_char,
    termios: *const Termios,
    window: *const Winsize,
) -> c_int {
    if master.is_null() || name.is_null() {
        return -1;
    }
    let mut buffer = [0u8; 128];
    // SAFETY: null or valid C records.
    let term = unsafe { termios.as_ref() };
    let win = unsafe { window.as_ref() };
    match raven_libc_compat::forkpty(&mut RealPty, Some(&mut buffer), term, win) {
        Ok((ForkResult::Parent(pid), master_fd)) => {
            // SAFETY: the out-pointers are writable.
            unsafe {
                *master = master_fd;
                super::write_cstr(
                    name,
                    &buffer[..super::c_len(buffer.as_ptr().cast(), buffer.len())],
                );
            }
            pid
        }
        Ok((ForkResult::Child, _)) => 0,
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            -1
        }
    }
}
