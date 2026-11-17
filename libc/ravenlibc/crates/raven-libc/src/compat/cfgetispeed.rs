// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `cfgetispeed`: the stored input line speed.

use raven_libc_abi::structs::Termios;
use raven_libc_ipc::termios;

/// `cfgetispeed`.
///
/// # Safety
/// `termios_p` is NULL or points to a readable `struct termios`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfgetispeed(termios_p: *const Termios) -> u32 {
    // SAFETY: the caller contract above says the pointer is NULL or readable.
    match unsafe { termios_p.as_ref() } {
        Some(value) => termios::cfgetispeed(value),
        None => 0,
    }
}
