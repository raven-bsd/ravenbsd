// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `tcgetsid`: the tty scheme has no session query, so the answer is `ENOSYS`.

use core::ffi::c_int;

use raven_libc_ipc::termios;

use super::fail;

/// `tcgetsid`.
///
/// # Safety
/// Scalar argument only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcgetsid(_fd: c_int) -> c_int {
    fail(termios::unsupported())
}
