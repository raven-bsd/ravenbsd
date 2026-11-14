// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![no_std]
#![no_main]

use core::arch::asm;
include!("alloc.rs");

#[repr(C)]
struct Tm {
    tm_sec: i32, tm_min: i32, tm_hour: i32, tm_mday: i32, tm_mon: i32, tm_year: i32,
    tm_wday: i32, tm_yday: i32, tm_isdst: i32, tm_gmtoff: i64, tm_zone: *mut u8,
}

unsafe fn write(bytes: &[u8]) {
    let result: isize;
    // SAFETY: Linux write(2) with a valid buffer.
    unsafe { asm!("syscall", in("rax") 1usize, in("rdi") 1usize, in("rsi") bytes.as_ptr(), in("rdx") bytes.len(), lateout("rax") result, lateout("rcx") _, lateout("r11") _, options(nostack)); }
    let _ = result;
}

unsafe fn exit(code: i32) -> ! {
    // SAFETY: Linux exit_group(2) never returns.
    unsafe { asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn)); }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let zero = 0i64;
    // SAFETY: the libc `localtime` and `strftime` contracts.
    let tm = unsafe { localtime(&zero) };
    if tm.is_null() {
        // SAFETY: Linux exit_group.
        unsafe { exit(1) };
    }
    let mut buffer = [0u8; 32];
    // SAFETY: the caller buffer and the C string format.
    let length = unsafe { strftime(buffer.as_mut_ptr(), buffer.len(), b"%Y-%m-%d\0".as_ptr(), tm) };
    if length != 10 || &buffer[..10] != b"1970-01-01" {
        // SAFETY: Linux exit_group.
        unsafe { exit(1) };
    }
    // SAFETY: fixed success text.
    unsafe { write(b"H3 OK\n") };
    // SAFETY: all checks passed.
    unsafe { exit(0) }
}

unsafe extern "C" {
    fn localtime(time: *const i64) -> *mut Tm;
    fn strftime(out: *mut u8, max: usize, format: *const u8, tm: *const Tm) -> usize;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: Linux exit_group.
    unsafe { exit(127) }
}
