// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S09 `/init` fixture: user threads and private futexes.
//!
//! Runs a 10,000-round two-word ping-pong through `FUTEX_WAIT`/`FUTEX_WAKE`, then checks
//! `-EAGAIN`, timeout `-ETIMEDOUT` and that a wake of three blocked waiters returns three.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, clock};

const ROUNDS: u32 = 10_000;
const STACK_SIZE: usize = 16 * 1024;

#[repr(align(16))]
struct Stack(core::cell::UnsafeCell<[u8; STACK_SIZE]>);

// SAFETY: each array is used as one thread's initial user stack and is never accessed through a
// Rust reference while the thread runs.
unsafe impl Sync for Stack {}

static STACK_PING: Stack = Stack(core::cell::UnsafeCell::new([0; STACK_SIZE]));
static STACK_WAIT_0: Stack = Stack(core::cell::UnsafeCell::new([0; STACK_SIZE]));
static STACK_WAIT_1: Stack = Stack(core::cell::UnsafeCell::new([0; STACK_SIZE]));
static STACK_WAIT_2: Stack = Stack(core::cell::UnsafeCell::new([0; STACK_SIZE]));

static PING: AtomicU32 = AtomicU32::new(0);
static PONG: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);

static EAGAIN_WORD: AtomicU32 = AtomicU32::new(7);
static TIMEOUT_WORD: AtomicU32 = AtomicU32::new(0);
static WAKE_WORD: AtomicU32 = AtomicU32::new(0);
static READY_0: AtomicU32 = AtomicU32::new(0);
static READY_1: AtomicU32 = AtomicU32::new(0);
static READY_2: AtomicU32 = AtomicU32::new(0);
static WAITERS_DONE: AtomicU32 = AtomicU32::new(0);

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
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
    // SAFETY: the S09 ABI fixes the syscall register assignment; the caller passes valid args.
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

#[inline(always)]
fn syscall1(number: usize, a0: usize) -> isize {
    // SAFETY: the register convention is the S09 ABI.
    unsafe { syscall6(number, a0, 0, 0, 0, 0, 0) }
}

#[inline(always)]
fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: the register convention is the S09 ABI.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
}

#[inline(always)]
fn syscall5(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    // SAFETY: the register convention is the S09 ABI.
    unsafe { syscall6(number, a0, a1, a2, a3, a4, 0) }
}

fn write(bytes: &[u8]) {
    let _ = syscall3(
        Syscall::DEBUG_WRITE as usize,
        bytes.as_ptr() as usize,
        bytes.len(),
        0,
    );
}

fn write_u32(mut value: u32) {
    let mut buffer = [0u8; 10];
    let mut index = buffer.len();
    if value == 0 {
        write(b"0");
        return;
    }
    while value > 0 {
        index -= 1;
        buffer[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    write(&buffer[index..]);
}

fn fail(name: &str, result: isize) -> ! {
    write(b"s09-futex: failed: ");
    write(name.as_bytes());
    write(b" result=");
    let mut buffer = [0u8; 18];
    buffer[0] = b'0';
    buffer[1] = b'x';
    let mut index = 2;
    let mut shift = 60i32;
    while shift >= 0 {
        let digit = ((result as usize >> shift) & 0xf) as u8;
        buffer[index] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        index += 1;
        shift -= 4;
    }
    write(&buffer);
    write(b"\n");
    exit(1)
}

fn futex_wait(word: &AtomicU32, expected: u32, timeout_ns: u64) -> isize {
    // SAFETY: the word is a live static and the ABI register convention is fixed.
    unsafe {
        syscall6(
            Syscall::FUTEX as usize,
            word as *const AtomicU32 as usize,
            0,
            expected as usize,
            timeout_ns as usize,
            0,
            0,
        )
    }
}

fn futex_wake(word: &AtomicU32, count: u32) -> isize {
    // SAFETY: the word is a live static and the ABI register convention is fixed.
    unsafe {
        syscall6(
            Syscall::FUTEX as usize,
            word as *const AtomicU32 as usize,
            1,
            count as usize,
            0,
            0,
            0,
        )
    }
}

fn now_ns() -> u64 {
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = syscall3(
        Syscall::CLOCK_GET as usize,
        clock::MONOTONIC as usize,
        &mut ts as *mut Timespec as usize,
        0,
    );
    if result != 0 {
        return 0;
    }
    let seconds = if ts.tv_sec < 0 { 0 } else { ts.tv_sec as u64 };
    let nanos = if ts.tv_nsec < 0 { 0 } else { ts.tv_nsec as u64 };
    seconds.saturating_mul(1_000_000_000).saturating_add(nanos)
}

fn nanosleep(ns: u64) {
    let _ = syscall1(Syscall::NANOSLEEP as usize, ns as usize);
}

fn yield_now() {
    let _ = syscall1(Syscall::YIELD as usize, 0);
}

fn stack_top(stack: &'static Stack) -> usize {
    let base = stack.0.get() as *const u8 as usize;
    base.saturating_add(STACK_SIZE)
}

fn spawn(entry: extern "C" fn() -> !, stack: &'static Stack) -> isize {
    syscall5(
        Syscall::THREAD_CREATE as usize,
        Handle::SELF_PROC.0 as usize,
        entry as usize,
        stack_top(stack),
        0,
        0,
    )
}

extern "C" fn ping_thread() -> ! {
    let mut round = 0u32;
    while round < ROUNDS {
        let result = futex_wait(&PING, 0, 0);
        if result != 0 && result != Errno::EAGAIN.to_ret() {
            fail("ping wait", result);
        }
        PING.store(0, Ordering::SeqCst);
        PONG.store(1, Ordering::SeqCst);
        let _ = futex_wake(&PONG, 1);
        round = round.saturating_add(1);
    }
    DONE.store(1, Ordering::SeqCst);
    exit_thread(0)
}

extern "C" fn waiter_0() -> ! {
    waiter(&READY_0)
}

extern "C" fn waiter_1() -> ! {
    waiter(&READY_1)
}

extern "C" fn waiter_2() -> ! {
    waiter(&READY_2)
}

fn waiter(ready: &AtomicU32) -> ! {
    ready.store(1, Ordering::SeqCst);
    let result = futex_wait(&WAKE_WORD, 0, 0);
    if result == 0 {
        WAITERS_DONE.fetch_add(1, Ordering::SeqCst);
    }
    exit_thread(0)
}

fn main() {
    let thread = spawn(ping_thread, &STACK_PING);
    if thread < 0 {
        fail("thread_create", thread);
    }

    let mut round = 0u32;
    while round < ROUNDS {
        PING.store(1, Ordering::SeqCst);
        let _ = futex_wake(&PING, 1);
        let result = futex_wait(&PONG, 0, 0);
        if result != 0 && result != Errno::EAGAIN.to_ret() {
            fail("pong wait", result);
        }
        PONG.store(0, Ordering::SeqCst);
        round = round.saturating_add(1);
    }

    let mut spin = 0u32;
    while DONE.load(Ordering::SeqCst) == 0 && spin < 1000 {
        yield_now();
        spin = spin.saturating_add(1);
    }
    write(b"futex: pingpong ok rounds=");
    write_u32(ROUNDS);
    write(b"\n");

    let result = futex_wait(&EAGAIN_WORD, 12345, 0);
    if result != Errno::EAGAIN.to_ret() {
        fail("eagain", result);
    }
    write(b"futex: eagain ok\n");

    let start = now_ns();
    let result = futex_wait(&TIMEOUT_WORD, 0, 50_000_000);
    let elapsed = now_ns().saturating_sub(start);
    if result != Errno::ETIMEDOUT.to_ret() || !(40_000_000..=1_000_000_000).contains(&elapsed) {
        fail("timeout", result);
    }
    write(b"futex: timeout ok\n");

    if spawn(waiter_0, &STACK_WAIT_0) < 0
        || spawn(waiter_1, &STACK_WAIT_1) < 0
        || spawn(waiter_2, &STACK_WAIT_2) < 0
    {
        fail("waiter create", -1);
    }
    let mut spin = 0u32;
    while (READY_0.load(Ordering::SeqCst) == 0
        || READY_1.load(Ordering::SeqCst) == 0
        || READY_2.load(Ordering::SeqCst) == 0)
        && spin < 10_000
    {
        yield_now();
        spin = spin.saturating_add(1);
    }
    // Let the waiters reach `FUTEX_WAIT` and block.
    nanosleep(20_000_000);
    WAKE_WORD.store(1, Ordering::SeqCst);
    let count = futex_wake(&WAKE_WORD, 10);
    if count != 3 {
        fail("wake count", count);
    }
    let mut spin = 0u32;
    while WAITERS_DONE.load(Ordering::SeqCst) < 3 && spin < 10_000 {
        nanosleep(1_000_000);
        spin = spin.saturating_add(1);
    }
    if WAITERS_DONE.load(Ordering::SeqCst) != 3 {
        fail("waiters done", WAITERS_DONE.load(Ordering::SeqCst) as isize);
    }
    write(b"futex: wake count 3\n");
    exit(0)
}

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

fn exit_thread(code: i32) -> ! {
    let _ = syscall1(Syscall::THREAD_EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

/// User entry point.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    main();
    exit(1)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
