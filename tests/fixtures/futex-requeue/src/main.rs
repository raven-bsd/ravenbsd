// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S203 futex-requeue fixture.
//!
//! Two private waiters block on the source word. `FUTEX_REQUEUE` with `wake = 1` and
//! `max = 1` wakes one waiter at the source and moves the other to the destination;
//! a second requeue on the empty source returns 0, a source wake returns 0, and the
//! destination wake returns 1. Both waiters return 0 and a second wake on either word
//! returns 0, so no waiter is lost or woken twice.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, futex};

const STACK_SIZE: usize = 16 * 1024;
const WAIT_TIMEOUT_NS: u64 = 2_000_000_000;
const READY_SPINS: u32 = 10_000;
const DONE_POLLS: u32 = 500;

#[repr(align(16))]
struct Stack(core::cell::UnsafeCell<[u8; STACK_SIZE]>);

// SAFETY: each array is used as one thread's initial user stack and is never accessed through a
// Rust reference while the thread runs.
unsafe impl Sync for Stack {}

static STACK_0: Stack = Stack(core::cell::UnsafeCell::new([0; STACK_SIZE]));
static STACK_1: Stack = Stack(core::cell::UnsafeCell::new([0; STACK_SIZE]));

static SOURCE: AtomicU32 = AtomicU32::new(0);
static TARGET: AtomicU32 = AtomicU32::new(0);
static READY: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
static RESULT: [AtomicI32; 2] = [AtomicI32::new(i32::MIN), AtomicI32::new(i32::MIN)];
static DONE: AtomicU32 = AtomicU32::new(0);

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
    // SAFETY: the x86-64 syscall register assignment is fixed by the ABI; the caller passes
    // scalar arguments and live user addresses.
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
    // SAFETY: the register convention is the native ABI.
    unsafe { syscall6(number, a0, 0, 0, 0, 0, 0) }
}

#[inline(always)]
fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: the register convention is the native ABI.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
}

#[inline(always)]
fn syscall5(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    // SAFETY: the register convention is the native ABI.
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

fn write_i32(value: i32) {
    if value < 0 {
        write(b"-");
        write_u32(value.unsigned_abs());
    } else {
        write_u32(value as u32);
    }
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

fn futex_wait(word: &AtomicU32, expected: u32) -> isize {
    // SAFETY: the word is a live static and the register convention is the native ABI.
    unsafe {
        syscall6(
            Syscall::FUTEX as usize,
            word as *const AtomicU32 as usize,
            (futex::WAIT | futex::PRIVATE) as usize,
            expected as usize,
            WAIT_TIMEOUT_NS as usize,
            0,
            0,
        )
    }
}

fn futex_wake(word: &AtomicU32, count: u32) -> isize {
    // SAFETY: the word is a live static and the register convention is the native ABI.
    unsafe {
        syscall6(
            Syscall::FUTEX as usize,
            word as *const AtomicU32 as usize,
            (futex::WAKE | futex::PRIVATE) as usize,
            count as usize,
            0,
            0,
            0,
        )
    }
}

fn futex_requeue(source: &AtomicU32, wake: u32, max: u32, target: &AtomicU32) -> isize {
    // SAFETY: both words are live statics and the register convention is the native ABI.
    // The fourth argument is `max` and the fifth is the destination address.
    unsafe {
        syscall6(
            Syscall::FUTEX as usize,
            source as *const AtomicU32 as usize,
            (futex::REQUEUE | futex::PRIVATE) as usize,
            wake as usize,
            max as usize,
            target as *const AtomicU32 as usize,
            0,
        )
    }
}

extern "C" fn waiter_0() -> ! {
    waiter(0)
}

extern "C" fn waiter_1() -> ! {
    waiter(1)
}

fn waiter(index: usize) -> ! {
    READY[index].store(1, Ordering::SeqCst);
    let result = futex_wait(&SOURCE, 0);
    RESULT[index].store(result as i32, Ordering::SeqCst);
    DONE.fetch_add(1, Ordering::SeqCst);
    exit_thread(0)
}

fn main() {
    if spawn(waiter_0, &STACK_0) < 0 || spawn(waiter_1, &STACK_1) < 0 {
        write(b"futex-requeue: FAIL spawn\n");
        exit(1);
    }
    let mut spin = 0u32;
    while (READY[0].load(Ordering::SeqCst) == 0 || READY[1].load(Ordering::SeqCst) == 0)
        && spin < READY_SPINS
    {
        yield_now();
        spin = spin.saturating_add(1);
    }
    if READY[0].load(Ordering::SeqCst) == 0 || READY[1].load(Ordering::SeqCst) == 0 {
        write(b"futex-requeue: FAIL armed\n");
        exit(1);
    }
    // Let both waiters execute FUTEX_WAIT and block.
    nanosleep(200_000_000);

    let requeue1 = futex_requeue(&SOURCE, 1, 1, &TARGET);
    let requeue2 = futex_requeue(&SOURCE, 0, 1, &TARGET);
    let wake_source = futex_wake(&SOURCE, 2);
    let wake_target = futex_wake(&TARGET, 2);
    let mut polls = 0u32;
    while DONE.load(Ordering::SeqCst) < 2 && polls < DONE_POLLS {
        nanosleep(1_000_000);
        polls = polls.saturating_add(1);
    }
    let extra_source = futex_wake(&SOURCE, 2);
    let extra_target = futex_wake(&TARGET, 2);
    let released = DONE.load(Ordering::SeqCst);
    let first = RESULT[0].load(Ordering::SeqCst);
    let second = RESULT[1].load(Ordering::SeqCst);

    write(b"futex-requeue: requeue1=");
    write_i32(requeue1 as i32);
    write(b" requeue2=");
    write_i32(requeue2 as i32);
    write(b" wake_source=");
    write_i32(wake_source as i32);
    write(b" wake_target=");
    write_i32(wake_target as i32);
    write(b" released=");
    write_u32(released);
    write(b"\n");

    let pass = requeue1 == 2
        && requeue2 == 0
        && wake_source == 0
        && wake_target == 1
        && extra_source == 0
        && extra_target == 0
        && released == 2
        && first == 0
        && second == 0;
    if !pass {
        write(b"futex-requeue: FAIL");
        write(b" extra_source=");
        write_i32(extra_source as i32);
        write(b" extra_target=");
        write_i32(extra_target as i32);
        write(b" first=");
        write_i32(first);
        write(b" second=");
        write_i32(second);
        write(b"\n");
        exit(1)
    }
    write(b"futex-requeue: done\n");
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
