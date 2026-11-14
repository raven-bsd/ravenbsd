// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P9: the generated `p9_all.rs` is the real probe; this copy documents the shape.
#![no_std]
#![no_main]
use core::ffi::c_int;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const u8, _envp: *const *const u8) -> c_int {
    0
}
