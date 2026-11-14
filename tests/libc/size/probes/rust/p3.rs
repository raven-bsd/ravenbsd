// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P3: printf("%f\n", argc * 1.5). The float plug-in must be pulled (R-13).
#![no_std]
#![no_main]
use core::ffi::{c_char, c_int};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

unsafe extern "C" {
    fn printf(format: *const c_char, ...) -> c_int;
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, _argv: *const *const c_char, _envp: *const *const c_char) -> c_int {
    unsafe {
        printf(c"%f\n".as_ptr(), argc as f64 * 1.5);
    }
    0
}
