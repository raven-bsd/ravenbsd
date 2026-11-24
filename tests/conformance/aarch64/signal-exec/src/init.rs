// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S197 aarch64 exec-default acceptance: install a caught `SIGUSR1` handler, then `exec` the
//! child image. The handler must not survive the exec; the child proves the reset and raises the
//! signal to prove the post-exec default action.
//!
//! The acceptance fixture is aarch64-only: the x86 C fixture in `tests/signals` uses the amd64
//! `REG_RIP`/`REG_RSP` ucontext and an x86-only build, so it cannot run on the aarch64 kernel.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::syscall::SIGACTION;
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const SIGUSR1: i32 = 10;

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

/// A real user function only so `sigaction` accepts the disposition; exec must clear it before
/// the child image runs, so it is never called.
extern "C" fn caught(_sig: i32) {}

/// The post-exec image, built first by `build.sh` and embedded so the service does not depend on
/// the file server being reachable from the initfs overlay.
const CHILD: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../build/conformance/aarch64/signal-exec/exec-child.elf"
));

fn main(_args: raven_rt::Args) -> i32 {
    println!("signal: exec-default start");
    let action = SigAction {
        handler: caught as *const () as u64,
        mask: [0; 16],
        flags: 0,
        _pad: 0,
        restorer: 0,
    };
    if unsafe {
        syscall3(
            SIGACTION,
            SIGUSR1 as usize,
            &action as *const SigAction as usize,
            0,
        )
    } != 0
    {
        println!("signal: fail sigaction");
        return 1;
    }
    println!("signal: exec-default armed");
    const ARGV: [&[u8]; 1] = [b"exec-child"];
    match raven_rt::exec::exec_image(CHILD, b"/exec-child", &ARGV, &[], &[]) {
        Ok(()) => {
            println!("signal: fail exec returned");
            1
        }
        Err(error) => {
            println!("signal: fail exec {}", error.to_ret());
            1
        }
    }
}
