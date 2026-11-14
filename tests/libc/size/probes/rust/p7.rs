// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P7: strftime("%c") + gmtime. Time without locale machinery (R-14).
#![no_std]
#![no_main]
use core::ffi::{c_char, c_int, c_long};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// The C `struct tm` as the headers define it (nine `int` fields, then two `long`).
#[repr(C)]
struct Tm {
    tm_sec: c_int,
    tm_min: c_int,
    tm_hour: c_int,
    tm_mday: c_int,
    tm_mon: c_int,
    tm_year: c_int,
    tm_wday: c_int,
    tm_yday: c_int,
    tm_isdst: c_int,
    tm_gmtoff: c_long,
    tm_zone: *const c_char,
}

unsafe extern "C" {
    fn gmtime(time: *const i64) -> *mut Tm;
    fn strftime(out: *mut c_char, max: usize, format: *const c_char, tm: *const Tm) -> usize;
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const u8, _envp: *const *const u8) -> c_int {
    let now: i64 = 1_700_000_000;
    let mut buffer = [0u8; 64];
    unsafe {
        let tm = gmtime(&now);
        if tm.is_null() {
            return 1;
        }
        let written = strftime(
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            c"%c".as_ptr(),
            tm,
        );
        if written == 0 && buffer[0] == 0 {
            return 2;
        }
    }
    0
}
