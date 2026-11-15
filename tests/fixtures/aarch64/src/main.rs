// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S56 aarch64 libc-gate parity fixture.
//!
//! Reproduces the S23/S54 kernel-side thread/TLS/futex/signal probes on aarch64 without the libc
//! harness or a C toolchain:
//!
//! 1. `THREAD_SET_TLS` must be effective for the caller before the next context switch; the
//!    fixture reads `TPIDR_EL0` and the marker at offset `0x31` immediately after the syscall.
//! 2. `THREAD_CREATE_EX` installs a per-thread `TPIDR_EL0`, writes `tid_out`, and clears
//!    `clear_tid_addr` on exit. The child reads its own TLS marker through `TPIDR_EL0`.
//! 3. Private `FUTEX_WAIT`/`FUTEX_WAKE` (including `-EAGAIN`/`-ETIMEDOUT`) and the clear-tid join.
//! 4. `SIG_SETUP`, a self `THREAD_KICK`, the registered aarch64 entry, the MContext at the
//!    registered TLS offset, and `SIGRETURN`.
//!
//! Any regression prints an `aarch64-libc: fail` line and exits nonzero; the qtest case treats
//! that as a failure.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::signal::{MContext, SIG_SETUP, SIGRETURN};
use raven_abi::syscall::{Syscall, THREAD_CREATE_EX, THREAD_KICK, THREAD_SET_TLS, ThreadCreateEx};
use raven_abi::types::{Handle, futex};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const TLS_LEN: usize = 2 * PAGE;
const STACK_PAGES: usize = 8;
const STACK_LEN: usize = STACK_PAGES * PAGE;
const TLS_OFFSET: usize = 0x31;
const SIGNAL_OFFSET: usize = 0x100;

const MAIN_MARKER: u64 = 0x5a5a_5a5a_5a5a_5a5a;
const CHILD_MARKER: u64 = 0xa5a5_a5a5_a5a5_a5a5;

const THREAD_WAITER: usize = usize::MAX;

const POLL_LIMIT: u32 = 4_000_000;
const WAIT_TIMEOUT_NS: u64 = 0;
const SLEEP_50_MS: u64 = 50_000_000;
const TIMEOUT_10_MS: u64 = 10_000_000;

static CHILD_TID: AtomicU32 = AtomicU32::new(0);
static CHILD_CLEAR: AtomicU32 = AtomicU32::new(1);
static CHILD_DONE: AtomicU32 = AtomicU32::new(0);
static CHILD_TLS_VALUE: AtomicU64 = AtomicU64::new(0);
static CHILD_TLS_ERROR: AtomicU32 = AtomicU32::new(0);

static WAITER_TID: AtomicU32 = AtomicU32::new(0);
static WAITER_CLEAR: AtomicU32 = AtomicU32::new(1);
static WAITER_READY: AtomicU32 = AtomicU32::new(0);
static WAITER_DONE: AtomicU32 = AtomicU32::new(0);
static WAITER_RESULT: AtomicU64 = AtomicU64::new(u64::MAX);

static FUTEX_WORD: AtomicU32 = AtomicU32::new(0);
static TIMEOUT_WORD: AtomicU32 = AtomicU32::new(7);

static SIGNAL_CONTEXT: AtomicU64 = AtomicU64::new(0);
static HANDLED: AtomicU32 = AtomicU32::new(0);
static HANDLER_BITS: AtomicU64 = AtomicU64::new(0);
static HANDLER_FAULT: AtomicU64 = AtomicU64::new(u64::MAX);
static HANDLER_MASK: AtomicU64 = AtomicU64::new(u64::MAX);
static HANDLER_PC: AtomicU64 = AtomicU64::new(0);

core::arch::global_asm!(
    r#"
    .section .text
    .global aarch64_libc_thread_entry
    .type aarch64_libc_thread_entry, %function
aarch64_libc_thread_entry:
    bl aarch64_libc_thread_body
    sxtw x0, w0
    mov x8, #0x21
    svc #0
    brk #0
    .size aarch64_libc_thread_entry, . - aarch64_libc_thread_entry

    .global aarch64_libc_signal_entry
    .type aarch64_libc_signal_entry, %function
aarch64_libc_signal_entry:
    bl aarch64_libc_signal_body
    brk #0
    .size aarch64_libc_signal_entry, . - aarch64_libc_signal_entry
"#
);

unsafe extern "C" {
    fn aarch64_libc_thread_entry();
    fn aarch64_libc_signal_entry();
}

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
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            options(nostack),
        );
    }
    result
}

fn sys(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
    unsafe { syscall6(number, a0, a1, a2, a3, a4, a5) }
}

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn map_region(len: usize) -> Result<usize, Errno> {
    sys::map_anonymous(len)
}

fn set_tls(base: usize) -> Result<(), Errno> {
    // SAFETY: the base is a mapped TLS range; the kernel validates the user address.
    result(sys(
        THREAD_SET_TLS,
        Handle::NONE.0 as usize,
        base,
        0,
        0,
        0,
        0,
    ))
    .map(|_| ())
}

fn create_thread(
    entry: unsafe extern "C" fn(),
    stack_top: usize,
    arg: usize,
    tp: usize,
    tid_out: &AtomicU32,
    clear_tid: &AtomicU32,
) -> Result<usize, Errno> {
    let extended = ThreadCreateEx {
        tp,
        tid_out: tid_out as *const AtomicU32 as usize,
        clear_tid_addr: clear_tid as *const AtomicU32 as usize,
    };
    // SAFETY: `extended` is a live payload; the kernel validates every address and the entry
    // lies in the fixture's executable text.
    result(sys(
        Syscall::THREAD_CREATE as usize,
        Handle::SELF_PROC.0 as usize,
        entry as *const () as usize,
        stack_top,
        arg,
        THREAD_CREATE_EX,
        &extended as *const ThreadCreateEx as usize,
    ))
}

fn futex_wait_word(word: &AtomicU32, expected: u32, timeout_ns: u64) -> Result<(), Errno> {
    // SAFETY: `word` is a live aligned AtomicU32; the kernel validates the user address.
    result(sys(
        Syscall::FUTEX as usize,
        word.as_ptr() as usize,
        (futex::WAIT | futex::PRIVATE) as usize,
        expected as usize,
        timeout_ns as usize,
        0,
        0,
    ))
    .map(|_| ())
}

fn futex_wake_word(word: &AtomicU32, count: u32) -> Result<usize, Errno> {
    // SAFETY: `word` is a live aligned AtomicU32; the kernel validates the user address.
    result(sys(
        Syscall::FUTEX as usize,
        word.as_ptr() as usize,
        (futex::WAKE | futex::PRIVATE) as usize,
        count as usize,
        0,
        0,
        0,
    ))
}

fn nanosleep_ns(ns: u64) {
    // SAFETY: NANOSLEEP takes only the scalar nanosecond duration.
    let _ = sys(Syscall::NANOSLEEP as usize, ns as usize, 0, 0, 0, 0, 0);
}

fn yield_now() {
    // SAFETY: YIELD takes no arguments.
    let _ = sys(Syscall::YIELD as usize, 0, 0, 0, 0, 0, 0);
}

fn sig_setup(entry: unsafe extern "C" fn(), offset: usize, flags: u32) -> Result<(), Errno> {
    // SAFETY: the entry is in the executable text and the context offset stays inside the
    // mapped TLS range; the kernel validates both.
    result(sys(
        SIG_SETUP,
        entry as *const () as usize,
        offset,
        flags as usize,
        0,
        0,
        0,
    ))
    .map(|_| ())
}

fn kick_self(bits: u32) -> Result<(), Errno> {
    // SAFETY: THREAD_KICK targets the calling thread with scalar bits.
    result(sys(
        THREAD_KICK,
        Handle::NONE.0 as usize,
        bits as usize,
        0,
        0,
        0,
        0,
    ))
    .map(|_| ())
}

/// The live user TLS base as installed by the kernel.
fn tls_base() -> usize {
    let base: usize;
    // SAFETY: reading TPIDR_EL0 has no side effects.
    unsafe {
        asm!("mrs {}, tpidr_el0", out(reg) base, options(nostack, preserves_flags));
    }
    base
}

fn read_tls_offset() -> u64 {
    let base = tls_base();
    // SAFETY: the caller has checked that `base` is the mapped TLS page; the read at offset
    // `0x31` is the same unaligned access libc performs.
    unsafe { core::ptr::read_unaligned((base + TLS_OFFSET) as *const u64) }
}

fn write_marker(page: usize, marker: u64) {
    // SAFETY: `page` is a mapped writable TLS page; the write at offset 0x31 is deliberately
    // unaligned to use the same offset as the libc TLS access.
    unsafe {
        core::ptr::write_unaligned((page + TLS_OFFSET) as *mut u64, marker);
        core::ptr::write_unaligned(page as *mut u64, page as u64);
    }
}

fn wait_for(word: &AtomicU32, value: u32) -> bool {
    let mut tries = 0;
    while word.load(Ordering::Acquire) != value {
        if tries == POLL_LIMIT {
            return false;
        }
        tries += 1;
        yield_now();
    }
    true
}

fn wait_for_zero(word: &AtomicU32) -> bool {
    wait_for(word, 0)
}

/// Shared body for the fixture's two user threads; `arg` selects the job.
#[unsafe(no_mangle)]
pub extern "C" fn aarch64_libc_thread_body(arg: usize) -> i32 {
    if arg == THREAD_WAITER {
        WAITER_READY.store(1, Ordering::Release);
        let status = match futex_wait_word(&FUTEX_WORD, 0, WAIT_TIMEOUT_NS) {
            Ok(()) => 0,
            Err(error) => u64::from(error.0),
        };
        WAITER_RESULT.store(status, Ordering::Release);
        WAITER_DONE.store(1, Ordering::Release);
        // Stay alive briefly so the parent can block in the clear-tid join wait.
        nanosleep_ns(SLEEP_50_MS);
        return 0;
    }

    // `arg` is the TLS page the kernel was asked to install for this thread.
    let base = tls_base();
    if base != arg {
        CHILD_TLS_ERROR.store(1, Ordering::Release);
    } else {
        CHILD_TLS_VALUE.store(read_tls_offset(), Ordering::Release);
    }
    CHILD_DONE.store(1, Ordering::Release);
    0
}

/// Signal entry body; the asm stub preserves the kernel's x0/x1/x2 argument registers.
#[unsafe(no_mangle)]
pub extern "C" fn aarch64_libc_signal_body(bits: u64, fault: u64, mask: u64) -> ! {
    let context = SIGNAL_CONTEXT.load(Ordering::Acquire) as *const MContext;
    // SAFETY: the kernel wrote an MContext at the registered TLS offset before the entry.
    let pc = unsafe { (*context).pc() };
    HANDLER_BITS.store(bits, Ordering::Release);
    HANDLER_FAULT.store(fault, Ordering::Release);
    HANDLER_MASK.store(mask, Ordering::Release);
    HANDLER_PC.store(pc, Ordering::Release);
    HANDLED.store(1, Ordering::Release);

    // SIGRETURN restores the interrupted frame; on success this never returns.
    let error: isize;
    // SAFETY: SIGRETURN takes no arguments and is valid inside a delivered signal.
    unsafe {
        asm!(
            "svc #0",
            in("x8") SIGRETURN,
            lateout("x0") error,
            options(nostack),
        );
    }
    println!("aarch64-libc: fail sigreturn {error}");
    sys::exit(1)
}

fn main(_args: raven_rt::Args) -> i32 {
    let main_tls = match map_region(TLS_LEN) {
        Ok(page) => page,
        Err(_) => {
            println!("aarch64-libc: fail map main tls");
            return 1;
        }
    };
    let child_tls = match map_region(TLS_LEN) {
        Ok(page) => page,
        Err(_) => {
            println!("aarch64-libc: fail map child tls");
            return 1;
        }
    };
    let child_stack = match map_region(STACK_LEN) {
        Ok(page) => page,
        Err(_) => {
            println!("aarch64-libc: fail map child stack");
            return 1;
        }
    };
    let waiter_stack = match map_region(STACK_LEN) {
        Ok(page) => page,
        Err(_) => {
            println!("aarch64-libc: fail map waiter stack");
            return 1;
        }
    };
    write_marker(main_tls, MAIN_MARKER);
    write_marker(child_tls, CHILD_MARKER);
    SIGNAL_CONTEXT.store((main_tls + SIGNAL_OFFSET) as u64, Ordering::Release);

    // S23 parity: THREAD_SET_TLS is effective for the caller before the next context switch.
    if set_tls(main_tls).is_err() {
        println!("aarch64-libc: fail set-tls");
        return 1;
    }
    let base = tls_base();
    if base != main_tls {
        println!("aarch64-libc: fail main tls base={base:#x} expected={main_tls:#x}");
        return 1;
    }
    if read_tls_offset() != MAIN_MARKER {
        println!("aarch64-libc: fail main tls marker");
        return 1;
    }

    // THREAD_CREATE_EX: per-thread tp, tid_out and clear_tid_addr.
    if create_thread(
        aarch64_libc_thread_entry,
        child_stack + STACK_LEN,
        child_tls,
        child_tls,
        &CHILD_TID,
        &CHILD_CLEAR,
    )
    .is_err()
    {
        println!("aarch64-libc: fail create");
        return 1;
    }
    if !wait_for(&CHILD_DONE, 1) {
        println!("aarch64-libc: fail child timeout");
        return 1;
    }
    if CHILD_TLS_ERROR.load(Ordering::Acquire) != 0 {
        println!("aarch64-libc: fail child tls base");
        return 1;
    }
    if CHILD_TLS_VALUE.load(Ordering::Acquire) != CHILD_MARKER {
        println!("aarch64-libc: fail child tls marker");
        return 1;
    }
    if CHILD_TID.load(Ordering::Acquire) == 0 {
        println!("aarch64-libc: fail child tid");
        return 1;
    }
    if !wait_for_zero(&CHILD_CLEAR) {
        println!("aarch64-libc: fail child clear");
        return 1;
    }
    println!("aarch64-libc: thread create ex ok");

    // FUTEX_WAIT/FUTEX_WAKE across the second thread.
    FUTEX_WORD.store(0, Ordering::Release);
    if create_thread(
        aarch64_libc_thread_entry,
        waiter_stack + STACK_LEN,
        THREAD_WAITER,
        0,
        &WAITER_TID,
        &WAITER_CLEAR,
    )
    .is_err()
    {
        println!("aarch64-libc: fail waiter create");
        return 1;
    }
    if !wait_for(&WAITER_READY, 1) {
        println!("aarch64-libc: fail waiter ready");
        return 1;
    }
    nanosleep_ns(SLEEP_50_MS);
    if WAITER_DONE.load(Ordering::Acquire) != 0 {
        println!("aarch64-libc: fail waiter not blocked");
        return 1;
    }
    FUTEX_WORD.store(1, Ordering::Release);
    match futex_wake_word(&FUTEX_WORD, 1) {
        Ok(1) => {}
        _ => {
            println!("aarch64-libc: fail wake count");
            return 1;
        }
    }
    if !wait_for(&WAITER_DONE, 1) {
        println!("aarch64-libc: fail wake timeout");
        return 1;
    }
    if WAITER_RESULT.load(Ordering::Acquire) != 0 {
        println!("aarch64-libc: fail wait result");
        return 1;
    }
    // The waiter sleeps 50 ms after signalling, so this join wait must block and be woken by
    // the kernel's clear-on-exit path, exactly like pthread_join.
    match futex_wait_word(&WAITER_CLEAR, 1, WAIT_TIMEOUT_NS) {
        Ok(()) | Err(Errno::EAGAIN) => {}
        _ => {
            println!("aarch64-libc: fail join wait");
            return 1;
        }
    }
    if !wait_for_zero(&WAITER_CLEAR) {
        println!("aarch64-libc: fail join clear");
        return 1;
    }
    println!("aarch64-libc: futex wait/wake ok");

    // A value mismatch is EAGAIN; a short timeout is ETIMEDOUT.
    if futex_wait_word(&FUTEX_WORD, 0, WAIT_TIMEOUT_NS) != Err(Errno::EAGAIN) {
        println!("aarch64-libc: fail futex eagain");
        return 1;
    }
    if futex_wait_word(&TIMEOUT_WORD, 7, TIMEOUT_10_MS) != Err(Errno::ETIMEDOUT) {
        println!("aarch64-libc: fail futex timeout");
        return 1;
    }
    println!("aarch64-libc: futex eagain ok");

    // Signal delivery through the registered aarch64 entry and SIGRETURN.
    if sig_setup(aarch64_libc_signal_entry, SIGNAL_OFFSET, 0).is_err() {
        println!("aarch64-libc: fail sig-setup");
        return 1;
    }
    if kick_self(1).is_err() {
        println!("aarch64-libc: fail kick");
        return 1;
    }
    if HANDLED.load(Ordering::Acquire) == 0 {
        println!("aarch64-libc: fail signal not delivered");
        return 1;
    }
    if HANDLER_BITS.load(Ordering::Acquire) != 1 {
        println!("aarch64-libc: fail signal bits");
        return 1;
    }
    if HANDLER_FAULT.load(Ordering::Acquire) != 0 {
        println!("aarch64-libc: fail signal fault pointer");
        return 1;
    }
    if HANDLER_MASK.load(Ordering::Acquire) != 0 {
        println!("aarch64-libc: fail signal mask");
        return 1;
    }
    let pc = HANDLER_PC.load(Ordering::Acquire);
    if !(0x40_0000..0x0000_8000_0000_0000).contains(&pc)
        || pc == aarch64_libc_signal_entry as *const () as usize as u64
    {
        println!("aarch64-libc: fail signal context pc={pc:#x}");
        return 1;
    }
    println!("aarch64-libc: signal delivery ok");

    println!("aarch64-libc: ok");
    0
}
