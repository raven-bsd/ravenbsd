// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P5: getaddrinfo(argv[1], "80", 0, &r). Resolver graph (3.A.4.4).
#![no_std]
#![no_main]
use core::ffi::{c_char, c_int, c_void};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

unsafe extern "C" {
    fn getaddrinfo(
        node: *const c_char,
        service: *const c_char,
        hints: *const c_void,
        result: *mut *mut c_void,
    ) -> c_int;
    fn freeaddrinfo(result: *mut c_void);
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char, _envp: *const *const c_char) -> c_int {
    let node = if argc > 1 {
        unsafe { *argv.add(1) }
    } else {
        c"127.0.0.1".as_ptr()
    };
    let mut result: *mut c_void = core::ptr::null_mut();
    unsafe {
        if getaddrinfo(node, c"80".as_ptr(), core::ptr::null(), &mut result) != 0 {
            return 1;
        }
        freeaddrinfo(result);
    }
    0
}
