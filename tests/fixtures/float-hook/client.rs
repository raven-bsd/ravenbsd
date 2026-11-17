// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! C74cu float-hook fixture: a static libc.a client that uses `%f` and `strfromd`.
//!
//! The client is linked by `check.sh` twice: with the driver's `-u __rl_fmt_float_impl`
//! request and without it. The readelf checks prove the strong plug-in and dtoa are pulled
//! only with the request; the tiny/static floor keeps the `*float*` dummy.

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

unsafe extern "C" {
    fn printf(format: *const c_char, ...) -> c_int;
    fn strfromd(s: *mut c_char, n: usize, format: *const c_char, value: f64) -> c_int;
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const u8, _envp: *const *const u8) -> c_int {
    let mut buffer = [0u8; 32];
    // SAFETY: the fixture owns its stdout stream and the stack buffer; the format strings are
    // NUL-terminated C literals, and the libc calls only read them and write the buffer.
    unsafe {
        printf(c"%f\n".as_ptr(), 1.5f64);
        strfromd(
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            c"%.2f".as_ptr(),
            1.5f64,
        );
    }
    0
}
