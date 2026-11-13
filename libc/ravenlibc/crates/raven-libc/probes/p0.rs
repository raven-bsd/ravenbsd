// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Probe P0: `_start` -> `exit(0)`; the floor of a static link.
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
    fn exit(status: i32) -> !;
}

#[unsafe(no_mangle)]
pub extern "C" fn probe_main() -> i32 {
    // SAFETY: exit never returns and takes one scalar.
    unsafe { exit(0) }
}
