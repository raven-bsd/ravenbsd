// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S712 exec probe: the image a caught-handler process replaces itself with.
//!
//! It is entered only through `PROC_EXEC`, so the kernel table it reads is the result of
//! `execsigs`: caught handlers are `SIG_DFL`, ignored dispositions stay ignored, the thread mask
//! is preserved, and the alternate stack is disabled. The probe reports the row and exits; it is
//! never the old image and never installs a handler.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::syscall::{SIGACTION, SIGALTSTACK, SIGPROCMASK};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const SIGUSR1: usize = 10;
const SIGUSR2: usize = 12;
const SIG_DFL: u64 = 0;
const SIG_IGN: u64 = 1;
const SS_DISABLE: u32 = 2;

/// The Raven private 40-byte `SIGACTION` request (`kernel/src/syscall/thread.rs`).
#[repr(C)]
#[derive(Clone, Copy)]
struct SigAction {
    handler: u64,
    mask: [u64; 2],
    flags: u32,
    _pad: u32,
    restorer: u64,
}

impl SigAction {
    const fn zero() -> Self {
        Self {
            handler: 0,
            mask: [0; 2],
            flags: 0,
            _pad: 0,
            restorer: 0,
        }
    }
}

/// System V amd64 `stack_t` (24 bytes).
#[repr(C)]
struct Stack {
    ss_sp: u64,
    ss_flags: i32,
    _pad: i32,
    ss_size: u64,
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            in("x2") a2,
            options(nostack),
        );
    }
    result
}

fn main(_args: raven_rt::Args) -> i32 {
    let mut us1 = SigAction::zero();
    let mut us2 = SigAction::zero();
    let mut mask = 0u64;
    let mut stack = Stack {
        ss_sp: 0,
        ss_flags: 0,
        _pad: 0,
        ss_size: 0,
    };
    // SAFETY: every object is live and each call only reads the kernel state (the act/ss
    // pointers are null).
    let queried = unsafe {
        syscall3(SIGACTION, SIGUSR1, 0, &mut us1 as *mut SigAction as usize) == 0
            && syscall3(SIGACTION, SIGUSR2, 0, &mut us2 as *mut SigAction as usize) == 0
            && syscall3(SIGPROCMASK, 0, 0, &mut mask as *mut u64 as usize) == 0
            && syscall3(SIGALTSTACK, 0, &mut stack as *mut Stack as usize, 0) == 0
    };
    let blocked_bit = 1u64 << (SIGUSR2 as u32 - 1);
    let reset = us1.handler == SIG_DFL;
    let ignored_kept = us2.handler == SIG_IGN;
    let mask_kept = mask & blocked_bit != 0;
    let altstack_off = stack.ss_flags as u32 & SS_DISABLE != 0;
    if queried && reset && ignored_kept && mask_kept && altstack_off {
        println!("signal: exec-inspect ok");
        0
    } else {
        println!(
            "signal: provider-fail exec-inspect queried={} us1={} us2={} mask=0x{:x} stack=0x{:x}",
            queried, us1.handler, us2.handler, mask, stack.ss_flags
        );
        1
    }
}
