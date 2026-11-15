// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S54 `libc-kernel` fixture.
//!
//! Reproduces the kernel half of the C73c libc-threads/libc-signals failures without the libc
//! harness or a C toolchain:
//!
//! 1. `THREAD_CREATE_EX` with a per-thread `tp`, a `tid_out` word and a `clear_tid_addr` word.
//!    The child reads its own TLS base at offset `0x31`, the same offset libc-threads faulted
//!    on; the parent observes the tid and the clear-on-exit word.
//! 2. Private `FUTEX_WAIT`/`FUTEX_WAKE` across two user threads, the `-EAGAIN` and `-ETIMEDOUT`
//!    contracts, and the clear-tid join wait that `pthread_join` uses.
//! 3. Signal delivery: `SIG_SETUP`, a self `THREAD_KICK`, the registered entry and `SIGRETURN`.
//!
//! Any regression in the kernel primitives that libc builds on prints a `libc-kernel: fail` line
//! and exits nonzero; the qtest case treats that as a failure.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::signal::{MContext, SIG_SETUP, SIGRETURN};
use raven_abi::syscall::{Syscall, THREAD_CREATE_EX, THREAD_KICK, THREAD_SET_TLS, ThreadCreateEx};
use raven_abi::types::{Handle, futex, map, prot};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const MAIN_TLS: usize = 0x5000_0000;
const CHILD_TLS: usize = 0x5000_1000;
const MAIN_STACK: usize = 0x6000_0000;
const WAITER_STACK: usize = 0x6000_8000;
const STACK_PAGES: usize = 8;
const STACK_LEN: usize = STACK_PAGES * PAGE;
const TLS_OFFSET: usize = 0x31;
const SIGNAL_OFFSET: usize = 0x100;

const MAIN_MARKER: u64 = 0x5a5a_5a5a_5a5a_5a5a;
const CHILD_MARKER: u64 = 0xa5a5_a5a5_a5a5_a5a5;

const THREAD_CHILD: usize = 1;
const THREAD_WAITER: usize = 2;

const POLL_LIMIT: u32 = 4_000_000;
const WAIT_TIMEOUT_NS: u64 = 0;
const SLEEP_50_MS: u64 = 50_000_000;
const TIMEOUT_10_MS: u64 = 10_000_000;

static CHILD_TID: AtomicU32 = AtomicU32::new(0);
static CHILD_CLEAR: AtomicU32 = AtomicU32::new(1);
static CHILD_DONE: AtomicU32 = AtomicU32::new(0);
static CHILD_TLS_VALUE: AtomicU64 = AtomicU64::new(0);

static WAITER_TID: AtomicU32 = AtomicU32::new(0);
static WAITER_CLEAR: AtomicU32 = AtomicU32::new(1);
static WAITER_READY: AtomicU32 = AtomicU32::new(0);
static WAITER_DONE: AtomicU32 = AtomicU32::new(0);
static WAITER_RESULT: AtomicU64 = AtomicU64::new(u64::MAX);

static FUTEX_WORD: AtomicU32 = AtomicU32::new(0);
static TIMEOUT_WORD: AtomicU32 = AtomicU32::new(7);

static HANDLED: AtomicU32 = AtomicU32::new(0);
static HANDLER_BITS: AtomicU64 = AtomicU64::new(0);
static HANDLER_FAULT: AtomicU64 = AtomicU64::new(u64::MAX);
static HANDLER_MASK: AtomicU64 = AtomicU64::new(u64::MAX);
static HANDLER_PC: AtomicU64 = AtomicU64::new(0);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global libc_kernel_thread_entry
    .type libc_kernel_thread_entry, @function
libc_kernel_thread_entry:
    call libc_kernel_thread_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size libc_kernel_thread_entry, . - libc_kernel_thread_entry

    .global libc_kernel_signal_entry
    .type libc_kernel_signal_entry, @function
libc_kernel_signal_entry:
    call libc_kernel_signal_body
    ud2
    .size libc_kernel_signal_entry, . - libc_kernel_signal_entry
"#
);

unsafe extern "C" {
    fn libc_kernel_thread_entry();
    fn libc_kernel_signal_entry();
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

fn map_region(address: usize, len: usize) -> Result<(), Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates the user range.
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            address,
            len,
            map::pack(prot::READ | prot::WRITE, map::FIXED),
        )
    })
    .map(|_| ())
}

fn set_tls(base: usize) -> Result<(), Errno> {
    // SAFETY: the base is a mapped TLS page; the kernel validates the user address.
    result(unsafe {
        syscall6(
            THREAD_SET_TLS,
            Handle::NONE.0 as usize,
            base,
            0,
            0,
            0,
            0,
        )
    })
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
    result(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            Handle::SELF_PROC.0 as usize,
            entry as *const () as usize,
            stack_top,
            arg,
            THREAD_CREATE_EX,
            &extended as *const ThreadCreateEx as usize,
        )
    })
}

fn futex_wait_word(word: &AtomicU32, expected: u32, timeout_ns: u64) -> Result<(), Errno> {
    // SAFETY: `word` is a live aligned AtomicU32; the kernel validates the user address.
    result(unsafe {
        syscall6(
            Syscall::FUTEX as usize,
            word.as_ptr() as usize,
            (futex::WAIT | futex::PRIVATE) as usize,
            expected as usize,
            timeout_ns as usize,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn futex_wake_word(word: &AtomicU32, count: u32) -> Result<usize, Errno> {
    // SAFETY: `word` is a live aligned AtomicU32; the kernel validates the user address.
    result(unsafe {
        syscall6(
            Syscall::FUTEX as usize,
            word.as_ptr() as usize,
            (futex::WAKE | futex::PRIVATE) as usize,
            count as usize,
            0,
            0,
            0,
        )
    })
}

fn nanosleep_ns(ns: u64) {
    // SAFETY: NANOSLEEP takes only the scalar nanosecond duration.
    let _ = unsafe { syscall6(Syscall::NANOSLEEP as usize, ns as usize, 0, 0, 0, 0, 0) };
}

fn yield_now() {
    // SAFETY: YIELD takes no arguments.
    let _ = unsafe { syscall6(Syscall::YIELD as usize, 0, 0, 0, 0, 0, 0) };
}

fn sig_setup(entry: unsafe extern "C" fn(), offset: usize, flags: u32) -> Result<(), Errno> {
    // SAFETY: the entry is in the executable text and the context offset stays inside the
    // mapped TLS page; the kernel validates both.
    result(unsafe {
        syscall6(
            SIG_SETUP,
            entry as *const () as usize,
            offset,
            flags as usize,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn kick_self(bits: u32) -> Result<(), Errno> {
    // SAFETY: THREAD_KICK targets the calling thread with scalar bits.
    result(unsafe {
        syscall6(
            THREAD_KICK,
            Handle::NONE.0 as usize,
            bits as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn read_tls_offset() -> u64 {
    let value: u64;
    // SAFETY: the caller installed a TLS base whose mapped page has a readable word at the
    // offset; the segment-base access is the point of the test.
    unsafe {
        asm!(
            "mov {}, qword ptr fs:[0x31]",
            out(reg) value,
            options(nostack, preserves_flags),
        );
    }
    value
}

fn write_marker(page: usize, marker: u64) {
    // SAFETY: `page` is a mapped writable page; the write at offset 0x31 is deliberately
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
pub extern "C" fn libc_kernel_thread_body(arg: usize) -> i32 {
    match arg {
        THREAD_CHILD => {
            CHILD_TLS_VALUE.store(read_tls_offset(), Ordering::Release);
            CHILD_DONE.store(1, Ordering::Release);
            0
        }
        THREAD_WAITER => {
            WAITER_READY.store(1, Ordering::Release);
            let status = match futex_wait_word(&FUTEX_WORD, 0, WAIT_TIMEOUT_NS) {
                Ok(()) => 0,
                Err(error) => u64::from(error.0),
            };
            WAITER_RESULT.store(status, Ordering::Release);
            WAITER_DONE.store(1, Ordering::Release);
            // Stay alive briefly so the parent can block in the clear-tid join wait.
            nanosleep_ns(SLEEP_50_MS);
            0
        }
        _ => 1,
    }
}

/// Signal entry body; the asm stub gives it a normal SysV frame.
#[unsafe(no_mangle)]
pub extern "C" fn libc_kernel_signal_body(bits: u64, fault: usize, mask: u64) -> ! {
    // SAFETY: the kernel wrote an MContext at the registered TLS offset before the entry.
    let pc = unsafe { (*((MAIN_TLS + SIGNAL_OFFSET) as *const MContext)).pc() };
    HANDLER_BITS.store(bits, Ordering::Release);
    HANDLER_FAULT.store(fault as u64, Ordering::Release);
    HANDLER_MASK.store(mask, Ordering::Release);
    HANDLER_PC.store(pc, Ordering::Release);
    HANDLED.store(1, Ordering::Release);

    // SIGRETURN restores the interrupted frame; on success this never returns.
    let error: isize;
    // SAFETY: SIGRETURN takes no arguments and is valid inside a delivered signal.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") SIGRETURN => error,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    println!("libc-kernel: fail sigreturn {error}");
    raven_rt::sys::exit(1)
}

fn main(_args: raven_rt::Args) -> i32 {
    if map_region(MAIN_TLS, PAGE).is_err()
        || map_region(CHILD_TLS, PAGE).is_err()
        || map_region(MAIN_STACK, STACK_LEN).is_err()
        || map_region(WAITER_STACK, STACK_LEN).is_err()
    {
        println!("libc-kernel: fail map");
        return 1;
    }
    write_marker(MAIN_TLS, MAIN_MARKER);
    write_marker(CHILD_TLS, CHILD_MARKER);

    // THREAD_SET_TLS is installed before the syscall returns (S23).
    if set_tls(MAIN_TLS).is_err() {
        println!("libc-kernel: fail set-tls");
        return 1;
    }
    if read_tls_offset() != MAIN_MARKER {
        println!("libc-kernel: fail main tls");
        return 1;
    }

    // THREAD_CREATE_EX: per-thread tp, tid_out and clear_tid_addr.
    if create_thread(
        libc_kernel_thread_entry,
        MAIN_STACK + STACK_LEN,
        THREAD_CHILD,
        CHILD_TLS,
        &CHILD_TID,
        &CHILD_CLEAR,
    )
    .is_err()
    {
        println!("libc-kernel: fail create");
        return 1;
    }
    if !wait_for(&CHILD_DONE, 1) {
        println!("libc-kernel: fail child timeout");
        return 1;
    }
    if CHILD_TLS_VALUE.load(Ordering::Acquire) != CHILD_MARKER {
        println!("libc-kernel: fail child tls");
        return 1;
    }
    if CHILD_TID.load(Ordering::Acquire) == 0 {
        println!("libc-kernel: fail child tid");
        return 1;
    }
    if !wait_for_zero(&CHILD_CLEAR) {
        println!("libc-kernel: fail child clear");
        return 1;
    }
    println!("libc-kernel: thread create ex ok");

    // FUTEX_WAIT/FUTEX_WAKE across the second thread.
    FUTEX_WORD.store(0, Ordering::Release);
    if create_thread(
        libc_kernel_thread_entry,
        WAITER_STACK + STACK_LEN,
        THREAD_WAITER,
        0,
        &WAITER_TID,
        &WAITER_CLEAR,
    )
    .is_err()
    {
        println!("libc-kernel: fail waiter create");
        return 1;
    }
    if !wait_for(&WAITER_READY, 1) {
        println!("libc-kernel: fail waiter ready");
        return 1;
    }
    nanosleep_ns(SLEEP_50_MS);
    if WAITER_DONE.load(Ordering::Acquire) != 0 {
        println!("libc-kernel: fail waiter not blocked");
        return 1;
    }
    FUTEX_WORD.store(1, Ordering::Release);
    match futex_wake_word(&FUTEX_WORD, 1) {
        Ok(1) => {}
        _ => {
            println!("libc-kernel: fail wake count");
            return 1;
        }
    }
    if !wait_for(&WAITER_DONE, 1) {
        println!("libc-kernel: fail wake timeout");
        return 1;
    }
    if WAITER_RESULT.load(Ordering::Acquire) != 0 {
        println!("libc-kernel: fail wait result");
        return 1;
    }
    // The waiter sleeps 50 ms after signalling, so this join wait must block and be woken by
    // the kernel's clear-on-exit path, exactly like pthread_join.
    match futex_wait_word(&WAITER_CLEAR, 1, WAIT_TIMEOUT_NS) {
        Ok(()) | Err(Errno::EAGAIN) => {}
        _ => {
            println!("libc-kernel: fail join wait");
            return 1;
        }
    }
    if !wait_for_zero(&WAITER_CLEAR) {
        println!("libc-kernel: fail join clear");
        return 1;
    }
    println!("libc-kernel: futex wait/wake ok");

    // A value mismatch is EAGAIN; a short timeout is ETIMEDOUT.
    if futex_wait_word(&FUTEX_WORD, 0, WAIT_TIMEOUT_NS) != Err(Errno::EAGAIN) {
        println!("libc-kernel: fail futex eagain");
        return 1;
    }
    if futex_wait_word(&TIMEOUT_WORD, 7, TIMEOUT_10_MS) != Err(Errno::ETIMEDOUT) {
        println!("libc-kernel: fail futex timeout");
        return 1;
    }
    println!("libc-kernel: futex eagain ok");

    // Signal delivery through the registered entry and SIGRETURN.
    if sig_setup(libc_kernel_signal_entry, SIGNAL_OFFSET, 0).is_err() {
        println!("libc-kernel: fail sig-setup");
        return 1;
    }
    if kick_self(1).is_err() {
        println!("libc-kernel: fail kick");
        return 1;
    }
    if HANDLED.load(Ordering::Acquire) == 0 {
        println!("libc-kernel: fail signal not delivered");
        return 1;
    }
    if HANDLER_BITS.load(Ordering::Acquire) != 1 {
        println!("libc-kernel: fail signal bits");
        return 1;
    }
    if HANDLER_FAULT.load(Ordering::Acquire) != 0 {
        println!("libc-kernel: fail signal fault pointer");
        return 1;
    }
    if HANDLER_MASK.load(Ordering::Acquire) != 0 {
        println!("libc-kernel: fail signal mask");
        return 1;
    }
    let pc = HANDLER_PC.load(Ordering::Acquire);
    if pc < 0x40_0000
        || pc >= 0x0000_8000_0000_0000
        || pc == libc_kernel_signal_entry as *const () as usize as u64
    {
        println!("libc-kernel: fail signal context pc");
        return 1;
    }
    println!("libc-kernel: signal delivery ok");

    println!("libc-kernel: ok");
    0
}
