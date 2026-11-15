// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `cfmakeraw(3)`.

use raven_libc_abi::structs::Termios;

/// Disable canonical mode, echo and signal generation on `termios`.
///
/// # Safety
/// `termios` is NULL or points to a writable `struct termios`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfmakeraw(termios: *mut Termios) {
    let Some(termios) = (unsafe { termios.as_mut() }) else {
        return;
    };
    raven_libc_platform::ioctl::tty::cfmakeraw(termios);
}
