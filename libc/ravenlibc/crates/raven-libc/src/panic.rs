// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one libc panic handler (feature `panic-handler`, R-6).
//!
//! It writes the fixed line `libc: panic` to descriptor 2 through the raw debug-write syscall
//! and calls the exported `abort` path. It must never format the panic payload: `core::fmt`
//! costs several kilobytes in every binary (F-6), and the handler must not allocate.

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_payload: &crate::ffi::PanicPayload<'_>) -> ! {
    const MESSAGE: &[u8] = b"libc: panic\n";
    // SAFETY: `DEBUG_WRITE` reads `MESSAGE` for its length and writes to the kernel's debug
    // sink; the bytes are a `'static` constant.
    unsafe {
        let _ = raven_libc_sys::arch::syscall3(0x03, 2, MESSAGE.as_ptr() as usize, MESSAGE.len());
    }
    // SAFETY: `abort` never returns and takes no arguments.
    crate::stdlib::abort::abort()
}
