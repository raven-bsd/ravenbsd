// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S24 `/child` fixture.
//!
//! Prints `child: hello <argv1>` through the C8 stack and exits 7. The child's initfs handle has
//! `READ|MAP` but no `EXEC`, so an `R|X` initfs mapping must fail with `EACCES`; this is the
//! rights negative test that cannot run in the parent (which needs EXEC to load this program).

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, prot};
use raven_rt::entry;

entry!(main);

const PAGE: usize = 4096;

#[inline(always)]
unsafe fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: the S24 ABI fixes the syscall register assignment; the caller passes valid args.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

fn write(bytes: &[u8]) {
    // SAFETY: the register convention is the S24 ABI and the byte slice is valid.
    let _ = unsafe {
        syscall6(
            Syscall::DEBUG_WRITE as usize,
            bytes.as_ptr() as usize,
            bytes.len(),
            0,
            0,
            0,
            0,
        )
    };
}

fn exit(code: i32) -> ! {
    // SAFETY: the register convention is the S24 ABI; EXIT takes one scalar argument.
    let _ = unsafe { syscall6(Syscall::EXIT as usize, code as usize, 0, 0, 0, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

fn main(args: raven_rt::Args) -> i32 {
    // The child's well-known initfs handle is READ|MAP (no EXEC); R|X must be refused.
    // SAFETY: the register convention is the S24 ABI; the kernel validates every argument.
    let result = unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::INITFS.0 as usize,
            0,
            0,
            PAGE,
            prot::READ | prot::EXEC,
        )
    };
    if result != Errno::EACCES.to_ret() {
        write(b"child: negative failed\n");
        return 8;
    }

    let argument = args.get(1).unwrap_or("world");
    if is_framecheck(argument) {
        return 7;
    }
    write(b"child: hello ");
    write(argument.as_bytes());
    write(b"\n");
    7
}

fn is_framecheck(text: &str) -> bool {
    let bytes = text.as_bytes();
    let expected = b"framecheck";
    if bytes.len() != expected.len() {
        return false;
    }
    let mut index = 0usize;
    while index < expected.len() {
        if bytes.get(index) != expected.get(index) {
            return false;
        }
        index += 1;
    }
    true
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
