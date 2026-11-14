// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![no_std]
#![no_main]

use core::arch::asm;

#[repr(C)]
struct Tm {
    tm_sec: i32, tm_min: i32, tm_hour: i32, tm_mday: i32, tm_mon: i32, tm_year: i32,
    tm_wday: i32, tm_yday: i32, tm_isdst: i32, tm_gmtoff: i64, tm_zone: *mut u8,
}

unsafe fn write(bytes: &[u8]) {
    let mut offset = 0usize;
    while offset < bytes.len() {
        let remaining = bytes.len() - offset;
        let result: isize;
        // SAFETY: Linux write(2) with a valid buffer and count.
        unsafe {
            asm!("syscall", in("rax") 1usize, in("rdi") 1usize, in("rsi") bytes.as_ptr().add(offset), in("rdx") remaining, lateout("rax") result, lateout("rcx") _, lateout("r11") _, options(nostack));
        }
        if result <= 0 { return; }
        offset += result as usize;
    }
}

unsafe fn exit(code: i32) -> ! {
    // SAFETY: Linux exit_group(2) never returns.
    unsafe { asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn)); }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut buffer = [0u8; 64];
    // SAFETY: the libc `snprintf` contract and a writable buffer.
    let length = unsafe { snprintf(buffer.as_mut_ptr(), buffer.len(), b"H1 %d\n\0".as_ptr(), 7i32) };
    if length != 5 {
        // SAFETY: Linux exit_group.
        unsafe { exit(1) };
    }
    // SAFETY: the buffer holds the formatted bytes.
    unsafe { write(&buffer[..5]) };
    // SAFETY: all checks passed.
    unsafe { exit(0) }
}

unsafe extern "C" {
    fn snprintf(buffer: *mut u8, capacity: usize, format: *const u8, ...) -> i32;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: Linux exit_group.
    unsafe { exit(127) }
}
