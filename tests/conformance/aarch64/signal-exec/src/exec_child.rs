// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S197 aarch64 exec-default child: the new image must have no handler for `SIGUSR1`; it queries
//! the disposition (must be `SIG_DFL`), then raises the signal and must die by the default action.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::syscall::{PROC_SIGQUEUE, SIGACTION};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const SIGUSR1: i32 = 10;
const SIG_DFL: u64 = 0;

/// System V `struct sigaction` (152 bytes; the kernel parser is architecture-neutral).
#[repr(C)]
struct SigAction {
    handler: u64,
    mask: [u64; 16],
    flags: i32,
    _pad: i32,
    restorer: u64,
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
    let mut previous = SigAction {
        handler: SIG_DFL,
        mask: [0; 16],
        flags: 0,
        _pad: 0,
        restorer: 0,
    };
    if unsafe {
        syscall3(
            SIGACTION,
            SIGUSR1 as usize,
            0,
            &mut previous as *mut SigAction as usize,
        )
    } != 0
    {
        println!("signal: fail sigaction-read");
        return 1;
    }
    if previous.handler != SIG_DFL {
        println!("signal: fail exec-reset handler={:#x}", previous.handler);
        return 1;
    }
    println!("signal: exec-reset ok");
    if unsafe { syscall3(PROC_SIGQUEUE, 0, SIGUSR1 as usize, 0) } != 0 {
        println!("signal: fail sigqueue");
        return 1;
    }
    println!("signal: fail exec-default survived");
    sys::thread_exit(1)
}
