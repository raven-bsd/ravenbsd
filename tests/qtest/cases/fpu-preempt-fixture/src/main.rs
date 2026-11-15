// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S22 FP/SIMD preemption fixture.
//!
//! Two user threads run dependent SSE `addsd` chains in different XMM registers and yield
//! periodically, so the timer and the syscall path switch between them while the FP state is
//! live. Each result is exact only if the kernel saves and restores the full x87/SSE state per
//! thread. A leaked XMM register or MXCSR state corrupts at least one result.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const STACK_ADDR: usize = 0x7000_0000;
const STACK_PAGES: usize = 8;
const STACK_LEN: usize = STACK_PAGES * PAGE;
const STACK_TOP: usize = STACK_ADDR + STACK_LEN;

/// Total dependent additions per thread.
const ITERATIONS: u64 = 20_000_000;
/// Additions between two `YIELD` syscalls.
const CHUNK: u64 = 100_000;
const CHUNKS: u64 = ITERATIONS / CHUNK;

const MAIN_START: u64 = 0.0f64.to_bits();
const MAIN_DELTA: u64 = 1.0f64.to_bits();
const MAIN_EXPECTED: u64 = 20_000_000.0f64.to_bits();

const HELPER_START: u64 = 1_000_000_000.0f64.to_bits();
const HELPER_DELTA: u64 = (-1.0f64).to_bits();

const HELPER_EXPECTED: u64 = 980_000_000.0f64.to_bits();

static HELPER_DONE: AtomicU32 = AtomicU32::new(0);
static HELPER_RESULT: AtomicU64 = AtomicU64::new(0);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global fpu_helper_entry
    .type fpu_helper_entry, @function
fpu_helper_entry:
    call fpu_helper_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size fpu_helper_entry, . - fpu_helper_entry
"#
);

unsafe extern "C" {
    fn fpu_helper_entry();
}

#[cfg(target_arch = "x86_64")]
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
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
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

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn map_stack() -> Result<(), Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates the address range.
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            STACK_ADDR,
            STACK_LEN,
            map::pack(prot::READ | prot::WRITE, map::FIXED),
        )
    })
    .map(|_| ())
}

fn thread_create(entry: usize, stack_top: usize) -> Result<Handle, Errno> {
    // SAFETY: the kernel validates the entry point and the user stack top.
    result(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            Handle::SELF_PROC.0 as usize,
            entry,
            stack_top,
            0,
            0,
            0,
        )
    })
    .map(|value| Handle(value as u32))
}

fn yield_now() {
    // SAFETY: YIELD takes no arguments.
    let _ = unsafe { syscall6(Syscall::YIELD as usize, 0, 0, 0, 0, 0, 0) };
}

/// Run `chunks * chunk` dependent `addsd` operations, yielding between chunks.
///
/// The XMM registers stay live across every `YIELD` syscall, so the syscall entry/exit and any
/// reschedule inside it must preserve them. The loop counters use r8/r9 and inputs r10/r12; the
/// syscall clobbers rcx/r11 only.
#[cfg(target_arch = "x86_64")]
fn add_chunked(start: u64, delta: u64, chunk: u64, chunks: u64) -> u64 {
    let output: u64;
    // SAFETY: the asm only touches the declared registers and the architecture's own XMM state;
    // the caller owns the thread's FP state and this thread is current.
    unsafe {
        asm!(
            "movq xmm0, {start}",
            "movq xmm1, {delta}",
            "mov r8, r10",
            "mov r9, r12",
            "2:",
            "addsd xmm0, xmm1",
            "dec r8",
            "jnz 2b",
            "mov eax, 2", // Syscall::YIELD
            "syscall",
            "mov r8, r10",
            "dec r9",
            "jnz 2b",
            "movq {output}, xmm0",
            start = in(reg) start,
            delta = in(reg) delta,
            in("r10") chunk,
            in("r12") chunks,
            output = lateout(reg) output,
            lateout("xmm0") _,
            lateout("xmm1") _,
            lateout("rax") _,
            lateout("rcx") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    output
}

/// Helper thread body: a different dependent SSE chain, then publish the result.
#[unsafe(no_mangle)]
pub extern "C" fn fpu_helper_body() -> i32 {
    let value = add_chunked(HELPER_START, HELPER_DELTA, CHUNK, CHUNKS);
    HELPER_RESULT.store(value, Ordering::Release);
    HELPER_DONE.store(1, Ordering::Release);
    0
}

fn main(_args: raven_rt::Args) -> i32 {
    if map_stack().is_err() {
        println!("fpu-preempt: fail stack map");
        return 1;
    }
    if thread_create(fpu_helper_entry as *const () as usize, STACK_TOP).is_err() {
        println!("fpu-preempt: fail helper thread");
        return 1;
    }

    let main_result = add_chunked(MAIN_START, MAIN_DELTA, CHUNK, CHUNKS);
    while HELPER_DONE.load(Ordering::Acquire) == 0 {
        yield_now();
    }
    let helper_result = HELPER_RESULT.load(Ordering::Acquire);
    if main_result != MAIN_EXPECTED {
        println!("fpu-preempt: fail main result");
        return 1;
    }
    if helper_result != HELPER_EXPECTED {
        println!(
            "fpu-preempt: fail helper result {:#x} expected {:#x}",
            helper_result, HELPER_EXPECTED
        );
        return 1;
    }
    println!("fpu-preempt: ok");
    0
}
