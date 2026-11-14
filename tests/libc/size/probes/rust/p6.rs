// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P6: dlopen(argv[1], RTLD_NOW). Static stub (R-16) or dynamic loader.
#![no_std]
#![no_main]
use core::ffi::{c_char, c_int, c_void};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

const RTLD_NOW: c_int = 2;

unsafe extern "C" {
    fn dlopen(file: *const c_char, mode: c_int) -> *mut c_void;
    fn dlerror() -> *const c_char;
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char, _envp: *const *const c_char) -> c_int {
    let file = if argc > 1 {
        unsafe { *argv.add(1) }
    } else {
        c"libnone.so".as_ptr()
    };
    unsafe {
        let handle = dlopen(file, RTLD_NOW);
        if handle.is_null() {
            let _ = dlerror();
            return 1;
        }
    }
    0
}
