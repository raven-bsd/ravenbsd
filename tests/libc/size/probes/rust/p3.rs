// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P3: printf("%f\n", argc * 1.5). The float plug-in must be pulled (R-13).
//
// The plug-in is an optional archive member (R-13/M2). A tiny program requests it exactly as
// the image link does with `-u __rl_fmt_float_impl`; without this reference the link keeps the
// `*float*` dummy and does not model the float program this probe exists for.
#![no_std]
#![no_main]
use core::ffi::{c_char, c_int};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

unsafe extern "C" {
    fn printf(format: *const c_char, ...) -> c_int;
    fn __rl_fmt_float_impl();
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, _argv: *const *const c_char, _envp: *const *const c_char) -> c_int {
    // A real float program requests the optional plug-in; `black_box` keeps the reference
    // live without the probe depending on a data-section retention rule.
    core::hint::black_box(__rl_fmt_float_impl as *const ());
    unsafe {
        printf(c"%f\n".as_ptr(), argc as f64 * 1.5);
    }
    0
}
