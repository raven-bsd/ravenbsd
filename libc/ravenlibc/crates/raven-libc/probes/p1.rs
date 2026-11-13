// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Probe P1: `write(1, "hi\n", 3)` then `exit(0)` (R-15).
#![no_std]
#![no_main]

core::arch::global_asm!(
    ".globl _start",
    "_start:",
    "xor rbp, rbp",
    "call probe_main",
    "mov edi, eax",
    "call exit",
);

unsafe extern "C" {
    fn write(fd: i32, buffer: *const u8, count: usize) -> isize;
    fn exit(status: i32) -> !;
}

#[unsafe(no_mangle)]
pub extern "C" fn probe_main() -> i32 {
    // SAFETY: the constant is three readable bytes.
    unsafe { write(1, b"hi\n".as_ptr(), 3) };
    // SAFETY: exit never returns.
    unsafe { exit(0) }
}
