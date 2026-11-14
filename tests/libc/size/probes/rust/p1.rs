// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P1: write(1, "hi\n", 3), no stdio (R-15).
#![no_std]
#![no_main]
use core::ffi::{c_int, c_void};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

unsafe extern "C" {
    fn write(fd: c_int, buf: *const c_void, n: usize) -> isize;
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const u8, _envp: *const *const u8) -> c_int {
    unsafe {
        write(1, b"hi\n".as_ptr().cast(), 3);
    }
    0
}
