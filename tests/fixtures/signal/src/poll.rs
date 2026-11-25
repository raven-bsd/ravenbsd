// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S44 poll-interrupt fixture: a caught kernel signal must interrupt an event-queue wait.
//!
//! The program installs a kernel `SA_SIGINFO` handler for SIGUSR1, registers an empty pipe read
//! handle on an event queue, and blocks in `EVQ_WAIT`. A helper thread queues SIGUSR1 to the
//! process. The only correct result is `EVQ_WAIT` returning `EINTR` with the handler having run;
//! a lost kernel kick leaves the wait to time out.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{PROC_SIGQUEUE, SIGACTION};
use raven_abi::types::{Handle, Timespec, clock};
use raven_rt::entry;
use raven_rt::event::{EventQueue, EvqEvent, READABLE};
use raven_rt::pipe;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const SIGUSR1: usize = 10;
const SA_SIGINFO: u32 = 4;
const WAIT_TIMEOUT_NS: u64 = 3_000_000_000;
const SIGNALLER_DELAY_NS: u64 = 200_000_000;

static HANDLED: AtomicU32 = AtomicU32::new(0);
static HANDLER_SIG: AtomicU32 = AtomicU32::new(0);
static WAIT_ARMED: AtomicU32 = AtomicU32::new(0);

/// The Raven private 40-byte `SIGACTION` request (C19; `kernel/src/syscall/thread.rs`).
#[repr(C)]
struct SigAction {
    handler: u64,
    mask: [u64; 2],
    flags: u32,
    _pad: u32,
    restorer: u64,
}

const _: () = assert!(core::mem::size_of::<SigAction>() == 40);
const _: () = assert!(core::mem::offset_of!(SigAction, handler) == 0);
const _: () = assert!(core::mem::offset_of!(SigAction, mask) == 8);
const _: () = assert!(core::mem::offset_of!(SigAction, flags) == 24);
const _: () = assert!(core::mem::offset_of!(SigAction, restorer) == 32);

#[repr(C, align(16))]
struct ThreadStack([u8; 64 * 1024]);

/// Writable `.bss` stack for the signaller thread. The kernel validates it before the thread runs.
static mut SIGNALLER_STACK: ThreadStack = ThreadStack([0; 64 * 1024]);

extern "C" fn handler(signo: i32) {
    HANDLED.fetch_add(1, Ordering::SeqCst);
    HANDLER_SIG.store(signo as u32, Ordering::SeqCst);
}

/// Queue SIGUSR1 once the main thread is inside the event wait.
extern "C" fn signaller(_arg: usize) -> ! {
    while WAIT_ARMED.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
    }
    let _ = sys::nanosleep(SIGNALLER_DELAY_NS);
    // SAFETY: scalar syscall arguments; target 0 is this process.
    let sent = unsafe { sys::syscall3(PROC_SIGQUEUE, 0, SIGUSR1, 0) };
    sys::thread_exit(if sent < 0 { 1 } else { 0 })
}

fn fail(row: &str, value: isize) -> i32 {
    println!("signal-poll: fail {} value={}", row, value);
    1
}

/// Nanoseconds from `start` to now; `u64::MAX` when the clock cannot be read.
fn elapsed_ns(start: Timespec) -> u64 {
    let Ok(now) = sys::clock_get(clock::MONOTONIC) else {
        return u64::MAX;
    };
    let seconds = now.tv_sec.saturating_sub(start.tv_sec).max(0) as u64;
    let nanos = now.tv_nsec.saturating_sub(start.tv_nsec).max(0) as u64;
    seconds.saturating_mul(1_000_000_000).saturating_add(nanos)
}

fn main(_args: raven_rt::Args) -> i32 {
    let action = SigAction {
        handler: handler as *const () as u64,
        mask: [0; 2],
        flags: SA_SIGINFO,
        _pad: 0,
        restorer: 0,
    };
    // SAFETY: the kernel copies the 40-byte request from the live stack object.
    let installed =
        unsafe { sys::syscall3(SIGACTION, SIGUSR1, &action as *const SigAction as usize, 0) };
    if installed < 0 {
        return fail("sigaction", installed);
    }

    let (read, _write) = match pipe::pipe() {
        Ok(handles) => handles,
        Err(error) => return fail("pipe", error.0 as isize),
    };
    let queue = match EventQueue::create() {
        Ok(queue) => queue,
        Err(error) => return fail("evq-create", error.0 as isize),
    };
    if let Err(error) = queue.add(read, READABLE, 1) {
        return fail("evq-add", error.0 as isize);
    }

    let stack_top =
        core::ptr::addr_of!(SIGNALLER_STACK) as usize + core::mem::size_of::<ThreadStack>();
    let thread = match sys::thread_create(
        Handle::SELF_PROC,
        signaller as *const () as usize,
        stack_top,
        0,
    ) {
        Ok(thread) => thread,
        Err(error) => return fail("thread-create", error.0 as isize),
    };

    let started = match sys::clock_get(clock::MONOTONIC) {
        Ok(start) => start,
        Err(error) => return fail("clock", error.0 as isize),
    };
    let mut events = [EvqEvent::default(); 1];
    WAIT_ARMED.store(1, Ordering::Release);
    let waited = queue.wait(&mut events, WAIT_TIMEOUT_NS);
    let _ = sys::handle_close(thread);
    match waited {
        Err(Errno::EINTR) => {
            if HANDLED.load(Ordering::SeqCst) != 1
                || HANDLER_SIG.load(Ordering::SeqCst) != SIGUSR1 as u32
            {
                return fail("handler", HANDLED.load(Ordering::SeqCst) as isize);
            }
            // A stale kick turned into EINTR only after the 3 s timeout would pass the result
            // check without proving the wake. The signal is sent about 200 ms after arming.
            let elapsed = elapsed_ns(started);
            if elapsed >= 1_000_000_000 {
                return fail("poll-latency", elapsed as isize);
            }
            println!("signal-poll: poll EINTR");
            println!("signal-poll: handler ok");
        }
        Err(error) => return fail("poll", error.0 as isize),
        Ok(count) => return fail("poll", count as isize),
    }
    println!("signal-poll: ok");
    sys::thread_exit(0)
}
