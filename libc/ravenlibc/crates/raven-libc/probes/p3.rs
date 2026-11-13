// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Probe P3: `printf("%f\n", argc * 1.5)`; must pull the float hook member (R-13).
#![no_std]
#![no_main]

core::arch::global_asm!(
    ".globl _start",
    "_start:",
    "xor rbp, rbp",
    "mov edi, DWORD PTR [rsp]",
    "call probe_main",
    "mov edi, eax",
    "call exit",
);

unsafe extern "C" {
    fn printf(format: *const u8, ...) -> i32;
    fn exit(status: i32) -> !;
}

#[unsafe(no_mangle)]
pub extern "C" fn probe_main(argc: i32) -> i32 {
    let value = f64::from(argc) * 1.5;
    // SAFETY: the format string is NUL terminated and the argument is a `double`.
    unsafe { printf(b"%f\n\0".as_ptr(), value) };
    // SAFETY: exit never returns.
    unsafe { exit(0) }
}
