// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S30 legacy-IRQ fixture.
//!
//! The controller holds `Handle::ROOT_DEVICE` (handle 3).  It proves the `irq_create` argument
//! register contract (flags in `r10`, `r8`/`r9` ignored), the single-owner GSI rule and the
//! masked-at-creation redirection entry.  The kernel prints the real IOAPIC mask bit when the
//! `s30-irq-probe` boot flag is set; this program only drives the syscalls.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const IRQ_LEGACY: u32 = 1;
/// ISA IRQ 0: the PIT line, always present and not used by the kernel's LAPIC timer.
const LEGACY_IRQ: u32 = 0;

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
    // SAFETY: the caller passes valid registers for the RavenBSD ABI; the kernel validates every
    // pointer and handle.
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

fn syscall1(number: usize, a0: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, 0, 0, 0, 0, 0) }
}

fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, 0, 0, 0, 0) }
}

fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn fail(message: &str) -> ! {
    println!("s30: failed {message}");
    exit(1)
}

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

fn nanosleep(ns: u64) {
    let _ = syscall1(Syscall::NANOSLEEP as usize, ns as usize);
}

fn handle_dup(handle: Handle, mask: u32) -> Result<Handle, Errno> {
    let raw = syscall2(
        Syscall::HANDLE_DUP as usize,
        handle.0 as usize,
        mask as usize,
    );
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn handle_close(handle: Handle) {
    let _ = syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize);
}

/// `irq_create` with every register spelled out: `flags` is the ABI's fourth argument (`r10`),
/// while `r8` and `r9` are not arguments and must be ignored by the kernel.
fn irq_create(
    root: Handle,
    kind: u32,
    number: u32,
    flags: u32,
    r8: usize,
    r9: usize,
) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::IRQ_CREATE as usize,
            root.0 as usize,
            kind as usize,
            number as usize,
            flags as usize,
            r8,
            r9,
        )
    })
}

fn irq_ack(irq: Handle) -> Result<(), Errno> {
    result(syscall1(Syscall::IRQ_ACK as usize, irq.0 as usize)).map(|_| ())
}

fn main(_args: raven_rt::Args) -> i32 {
    let admin = match handle_dup(Handle::ROOT_DEVICE, Rights::ADMIN.union(Rights::DUP).0) {
        Ok(handle) => handle,
        Err(_) => fail("root dup"),
    };

    // I1: the legacy branch used to test r8 for flags.  A zero r10 with junk in r8/r9 must
    // create the line; before the fix this exact call returned EINVAL.
    let first = match irq_create(admin, IRQ_LEGACY, LEGACY_IRQ, 0, 0xDEAD_BEEF, 0xFEED_FACE) {
        Ok(handle) => handle,
        Err(error) => {
            println!("s30: failed legacy create errno={}", error.0);
            return 60;
        }
    };
    println!("s30: legacy create ok");

    // A nonzero fourth argument (r10) is EINVAL, whatever r8/r9 hold.
    if irq_create(admin, IRQ_LEGACY, LEGACY_IRQ, 1, 0, 0) != Err(Errno::EINVAL) {
        fail("legacy flags accepted");
    }
    println!("s30: legacy flags ok");

    // The GSI already has an owner: a second object must fail with EBUSY.
    if irq_create(admin, IRQ_LEGACY, LEGACY_IRQ, 0, 0, 0) != Err(Errno::EBUSY) {
        fail("duplicate gsi accepted");
    }
    println!("s30: legacy busy ok");

    // The line stays masked while the first object lives.  Sleep long enough that an unmasked
    // PIT line would have fired; the kernel probe prints the real redirection mask bit at
    // creation (masked=1) and again after the ack (masked=0).
    nanosleep(50_000_000);

    // Closing releases both the GSI claim and the vector; a fresh create succeeds.
    handle_close(first);
    let second = match irq_create(admin, IRQ_LEGACY, LEGACY_IRQ, 0, 0, 0) {
        Ok(handle) => handle,
        Err(error) => {
            println!("s30: failed legacy reuse errno={}", error.0);
            return 61;
        }
    };
    println!("s30: legacy reuse ok");

    // IRQ_ACK is the documented unmask point for a line that never waited.
    if irq_ack(second).is_err() {
        fail("legacy ack");
    }
    println!("s30: legacy ack ok");

    handle_close(second);
    println!("s30: legacy done");
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
