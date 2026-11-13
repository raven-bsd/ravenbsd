// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `abort(3)`.
//!
//! The default output is the fixed line on descriptor 2 and the C abort exit status; no signal
//! is raised because the signal area is C58's and the kernel signal set is not wired yet.

/// Terminate abnormally.
#[unsafe(no_mangle)]
pub extern "C" fn abort() -> ! {
    const MESSAGE: &[u8] = b"libc: abort\n";
    // SAFETY: `DEBUG_WRITE` reads the constant for its length.
    unsafe {
        let _ = raven_libc_sys::arch::syscall3(0x03, 2, MESSAGE.as_ptr() as usize, MESSAGE.len());
    }
    raven_libc_sys::sys::exit(134)
}
