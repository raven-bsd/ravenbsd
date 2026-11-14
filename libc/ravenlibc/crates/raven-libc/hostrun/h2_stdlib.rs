// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Host-run h2: `<stdlib.h>` conversions and one `snprintf` double format.
//!
//! The exit code is the index of the first failed check (C60 section 4.8). The float check is
//! last: until C61's start path installs the `__rl_fmt_float` hook, bare-`_start` programs get
//! the documented `*float*` placeholder, and the C59 variadic ABI is unverified on
//! `x86_64-unknown-none`; the report records the finding instead of hiding it.
#![no_std]
#![no_main]

use core::arch::asm;

unsafe fn write(bytes: &[u8]) {
    let result: isize;
    // SAFETY: Linux write(2) with a valid buffer.
    unsafe {
        asm!("syscall", in("rax") 1usize, in("rdi") 1usize, in("rsi") bytes.as_ptr(), in("rdx") bytes.len(), lateout("rax") result, lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    let _ = result;
}

unsafe fn exit(code: i32) -> ! {
    // SAFETY: Linux exit_group(2) never returns.
    unsafe { asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn)) }
}

fn fail(index: i32) -> ! {
    let mut text = *b"H2 FAIL 0\n";
    if let Some(byte) = text.get_mut(8) {
        *byte = b'0' + u8::try_from(index).unwrap_or(0);
    }
    // SAFETY: fixed local buffer.
    unsafe { write(&text) };
    // SAFETY: Linux exit_group.
    unsafe { exit(index) }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // SAFETY: the C `<stdlib.h>` contracts.
    unsafe {
        if atoi(b"-42\0".as_ptr()) != -42 {
            fail(1);
        }
        let mut end = core::ptr::null_mut();
        if strtol(b"0x1f\0".as_ptr(), &mut end, 16) != 31 {
            fail(2);
        }
        if strtol(b"9223372036854775808\0".as_ptr(), &mut end, 10) != i64::MAX {
            fail(3);
        }
        if abs(-7) != 7 {
            fail(4);
        }
        let mut buffer = [0u8; 64];
        let length = snprintf(buffer.as_mut_ptr(), buffer.len(), b"%.3f\0".as_ptr(), 1.5f64);
        if length != 5 || &buffer[..5] != b"1.500" {
            // SAFETY: diagnostic write of the real formatting result.
            unsafe {
                write(b"H2 float: ");
                write(&buffer[..usize::try_from(length).unwrap_or(0).min(buffer.len())]);
                write(b"\n");
            }
            fail(5);
        }
    }
    // SAFETY: fixed success text.
    unsafe { write(b"H2 OK\n") };
    // SAFETY: all checks passed.
    unsafe { exit(0) }
}

unsafe extern "C" {
    fn atoi(text: *const u8) -> i32;
    fn strtol(text: *const u8, end: *mut *mut u8, base: i32) -> i64;
    fn abs(value: i32) -> i32;
    fn snprintf(buffer: *mut u8, capacity: usize, format: *const u8, ...) -> i32;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: Linux exit_group.
    unsafe { exit(127) }
}
