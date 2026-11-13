// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the login_tty export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `login_tty` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};
use raven_libc_abi::generated::structs::Termios;
use raven_libc_compat::Winsize;

use super::pty_state::RealPty;

/// `login_tty`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn login_tty(fd: c_int) -> c_int {
    match raven_libc_compat::login_tty(&mut RealPty, fd) {
        Ok(()) => 0,
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            -1
        }
    }
}
