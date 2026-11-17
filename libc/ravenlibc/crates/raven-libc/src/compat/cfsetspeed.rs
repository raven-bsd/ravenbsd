// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `cfsetspeed`: validate and store both line speeds.

use core::ffi::c_int;

use raven_libc_abi::structs::Termios;
use raven_libc_ipc::termios;

use super::tty_ipc_state::set_speed;

/// `cfsetspeed`.
///
/// # Safety
/// `termios_p` is NULL or points to a writable `struct termios`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfsetspeed(termios_p: *mut Termios, speed: u32) -> c_int {
    // SAFETY: the caller contract above grants a readable and writable record (or NULL).
    unsafe { set_speed(termios_p, speed, termios::cfsetspeed) }
}
