// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S26 `/init` fixture: user-event delivery, a blocking wake, and a monotonic timer.

#![no_std]
#![no_main]

use core::arch::asm;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, clock};

const ADD: usize = 1;
const DEL: usize = 3;
const SIGNALED: usize = 8;
const STACK_SIZE: usize = 16 * 1024;
const EVENT_ROUNDS: usize = 100_000;

#[repr(C)]
struct EvqEvent {
    cookie: u64,
    events: u32,
    pad: u32,
}

#[repr(align(16))]
struct Stack(UnsafeCell<[u8; STACK_SIZE]>);

// SAFETY: this static is only used as the child thread's initial stack and is not accessed while
// that thread runs.
unsafe impl Sync for Stack {}

static SIGNAL_STACK: Stack = Stack(UnsafeCell::new([0; STACK_SIZE]));
static EVENT_HANDLE: AtomicU32 = AtomicU32::new(u32::MAX);

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
    // SAFETY: RavenBSD's x86-64 ABI fixes these argument registers for every syscall.
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

fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
}

fn syscall4(number: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    // SAFETY: the event output pointer below names a live `EvqEvent` array.
    unsafe { syscall6(number, a0, a1, a2, a3, 0, 0) }
}

fn syscall5(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, a2, a3, a4, 0) }
}

fn write(bytes: &[u8]) {
    let _ = syscall2(
        Syscall::DEBUG_WRITE as usize,
        bytes.as_ptr() as usize,
        bytes.len(),
    );
}

fn fail(message: &[u8]) -> ! {
    write(b"s26: failed ");
    write(message);
    write(b"\n");
    exit(1)
}

fn handle(result: isize) -> Handle {
    if result < 0 || result > isize::try_from(u32::MAX).unwrap_or(isize::MAX) {
        fail(b"handle");
    }
    Handle(result as u32)
}

fn ctl(queue: Handle, operation: usize, source: Handle, events: usize, cookie: u64) {
    if syscall5(
        Syscall::EVQ_CTL as usize,
        queue.0 as usize,
        operation,
        source.0 as usize,
        events,
        cookie as usize,
    ) != 0
    {
        fail(b"ctl");
    }
}

fn wait(queue: Handle, timeout_ns: usize) -> EvqEvent {
    let mut event = EvqEvent {
        cookie: 0,
        events: 0,
        pad: 0,
    };
    let count = syscall4(
        Syscall::EVQ_WAIT as usize,
        queue.0 as usize,
        (&mut event as *mut EvqEvent) as usize,
        1,
        timeout_ns,
    );
    if count != 1 {
        if count == 0 {
            fail(b"wait zero");
        }
        fail(b"wait error");
    }
    event
}

extern "C" fn signal_thread() -> ! {
    let _ = syscall1(Syscall::NANOSLEEP as usize, 1_000_000);
    let event = EVENT_HANDLE.load(Ordering::Acquire);
    if syscall2(Syscall::EVENT_SIGNAL as usize, event as usize, 0) != 0 {
        exit_thread(1);
    }
    exit_thread(0)
}

fn stack_top(stack: &'static Stack) -> usize {
    (stack.0.get() as *const u8 as usize).saturating_add(STACK_SIZE)
}

fn main() {
    let queue = handle(syscall1(Syscall::EVQ_CREATE as usize, 0));
    let event = handle(syscall1(Syscall::EVENT_CREATE as usize, 0));
    EVENT_HANDLE.store(event.0, Ordering::Release);
    ctl(queue, ADD, event, SIGNALED, 0x51);
    let thread = syscall5(
        Syscall::THREAD_CREATE as usize,
        Handle::SELF_PROC.0 as usize,
        signal_thread as *const () as usize,
        stack_top(&SIGNAL_STACK),
        0,
        0,
    );
    if thread < 0 {
        fail(b"thread");
    }
    let blocked = wait(queue, 100_000_000);
    if blocked.cookie != 0x51 || blocked.events != SIGNALED as u32 {
        fail(b"blocking event");
    }

    let mut round = 0usize;
    while round < EVENT_ROUNDS {
        if syscall2(Syscall::EVENT_SIGNAL as usize, event.0 as usize, 0) != 0 {
            fail(b"signal");
        }
        let delivered = wait(queue, 1_000_000);
        if delivered.cookie != 0x51 || delivered.events != SIGNALED as u32 {
            fail(b"event delivery");
        }
        round = round.saturating_add(1);
    }

    ctl(queue, DEL, event, 0, 0);
    let timer = handle(syscall2(
        Syscall::TIMER_CREATE as usize,
        clock::MONOTONIC as usize,
        0,
    ));
    ctl(queue, ADD, timer, SIGNALED, 0x26);
    if syscall3(
        Syscall::TIMER_SET as usize,
        timer.0 as usize,
        10_000_000,
        10_000_000,
    ) != 0
    {
        fail(b"timer set");
    }
    let expired = wait(queue, 100_000_000);
    if expired.cookie != 0x26 || expired.events != SIGNALED as u32 {
        fail(b"timer delivery");
    }
    write(b"s26: requests=0 timer=1 events=100000 lost=0\n");
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
