// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K543 readiness contract fixture.
//!
//! Three rows exercise the event-core contract against NetBSD `sys_select.c` semantics:
//!
//! * lost wake: a signaller reacts to a round marker written immediately before `EVQ_WAIT`, so
//!   the signal lands between the entry scan and the sleep. `wait_once` re-checks readiness under
//!   the wait lock (`selrecord`), so the waiter must never time out.
//! * HUP re-poll: closing the pipe writer leaves the read end at `READABLE|HUP`; a second wait
//!   must still report `HUP` (`seltrue` is level-true, never cached).
//! * EINTR re-evaluation: `THREAD_KICK` interrupts a blocked wait; after the event is signalled the
//!   retry must observe the live level (`sel_setevents`/`SEL_RESET`).
//!
//! The fixture is a guest proof; the mutating negative checks run against the kernel event core.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw syscall for THREAD_KICK and the thread stack; each block has its SAFETY note.

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_KICK};
use raven_abi::types::{Handle, clock};
use raven_rt::event::{EventQueue, EvqEvent, HUP, READABLE, SIGNALED, event_create, event_signal};
use raven_rt::pipe::pipe;
use raven_rt::thread;
use raven_rt::{clock_get, entry, handle_close, nanosleep, println, sys};

entry!(main);

/// Rounds where the signal is fired between the entry scan and the sleep.
const LOST_WAKE_ROUNDS: u32 = 2000;
/// Registrations scanned before the enqueue. The target event is first, so the signal fired
/// while the scan walks these later registrations lands after the target's readiness evaluation
/// and before `wait_once` publishes the waiter.
const SCAN_FILLER: usize = 255;
/// Largest signaller spin delay after the round marker, in `spin_loop` iterations.
const MAX_SIGNAL_DELAY: u32 = 20_000;
/// A source wake is direct; a lost wake is rescued only by the next 1 ms timer tick. This ceiling
/// sits below the tick so the two are distinguishable.
const WAKE_LATENCY_LIMIT_NS: u64 = 900_000;
/// Host scheduling noise can push a direct wake past the ceiling a few times; a lost wake hits
/// the tick on a large fraction of the rounds.
const SLOW_WAKE_LIMIT: u32 = 10;
/// Timeout for one lost-wake round; a lost wake shows as a zero result.
const ROUND_TIMEOUT_NS: u64 = 200_000_000;
/// Timeout for one EINTR retry.
const EINTR_TIMEOUT_NS: u64 = 300_000_000;
/// Delay before kicking the blocked waiter.
const KICK_DELAY_NS: u64 = 50_000_000;
/// User thread stack size.
const STACK_SIZE: usize = 64 * 1024;
/// `THREAD_KICK` bit: an arbitrary non-zero kick makes the next interruptible wait return EINTR.
const KICK_BIT: usize = 1;

static ROUND: AtomicU32 = AtomicU32::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
static SIGNAL_EVENT: AtomicU32 = AtomicU32::new(u32::MAX);
static SIGNAL_ERROR: AtomicU32 = AtomicU32::new(0);

static EINTR_QUEUE: AtomicU32 = AtomicU32::new(u32::MAX);
static EINTR_EVENT: AtomicU32 = AtomicU32::new(u32::MAX);
static EINTR_SAW: AtomicU32 = AtomicU32::new(0);
static EINTR_DONE: AtomicI32 = AtomicI32::new(0);

type R = Result<(), &'static str>;

fn signaller(_: usize) {
    let event = Handle(SIGNAL_EVENT.load(Ordering::Acquire));
    let mut seen = u32::MAX;
    while !STOP.load(Ordering::Acquire) {
        let round = ROUND.load(Ordering::Acquire);
        if round != seen {
            seen = round;
            // Sweep the delay across the entry scan: some rounds fire before `collect_ready`,
            // some in the scan tail after the target was evaluated, some after the enqueue.
            for _ in 0..(round % MAX_SIGNAL_DELAY) {
                core::hint::spin_loop();
            }
            if event_signal(event, 1).is_err() {
                SIGNAL_ERROR.store(1, Ordering::Release);
            }
        }
        core::hint::spin_loop();
    }
}

fn now_ns() -> u64 {
    let Ok(now) = clock_get(clock::MONOTONIC) else {
        return 0;
    };
    (now.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(now.tv_nsec as u64)
}

fn lost_wake() -> R {
    let queue = EventQueue::create().map_err(|_| "queue")?;
    let event = event_create().map_err(|_| "event")?;
    queue.add(event, SIGNALED, 1).map_err(|_| "add")?;
    // Fill the registration table with idle user events. They are never signalled, so only the
    // target event can make the queue ready; they only widen the entry scan.
    for _ in 0..SCAN_FILLER {
        let filler = event_create().map_err(|_| "filler")?;
        queue.add(filler, SIGNALED, 0).map_err(|_| "add filler")?;
    }
    SIGNAL_EVENT.store(event.0, Ordering::Release);
    let worker = thread::spawn(signaller, 0, STACK_SIZE).map_err(|_| "thread")?;

    let mut output = [EvqEvent::default(); 1];
    let mut timeouts = 0u32;
    let mut slow = 0u32;
    let mut max_latency = 0u64;
    for round in 1..=LOST_WAKE_ROUNDS {
        ROUND.store(round, Ordering::Release);
        let started = now_ns();
        match queue.wait(&mut output, ROUND_TIMEOUT_NS) {
            Ok(1) if output[0].cookie == 1 && output[0].events & SIGNALED != 0 => {
                let elapsed = now_ns().saturating_sub(started);
                max_latency = max_latency.max(elapsed);
                if elapsed >= WAKE_LATENCY_LIMIT_NS {
                    slow = slow.saturating_add(1);
                }
            }
            Ok(0) => {
                timeouts = timeouts.saturating_add(1);
                break;
            }
            _ => break,
        }
    }
    STOP.store(true, Ordering::Release);
    worker.join().map_err(|_| "join")?;
    if SIGNAL_ERROR.load(Ordering::Acquire) != 0 {
        return Err("signaller");
    }
    if timeouts != 0 {
        return Err("lost wake");
    }
    if slow > SLOW_WAKE_LIMIT {
        return Err("tick-rescued wake");
    }
    println!(
        "readiness: lost-wake rounds={} timeouts=0 max_us={} slow={} ok",
        LOST_WAKE_ROUNDS,
        max_latency / 1_000,
        slow
    );
    Ok(())
}

fn hup_repoll() -> R {
    let queue = EventQueue::create().map_err(|_| "queue")?;
    let (read, write) = pipe().map_err(|_| "pipe")?;
    queue.add(read, READABLE | HUP, 2).map_err(|_| "add pipe")?;
    handle_close(write).map_err(|_| "close writer")?;

    let mut output = [EvqEvent::default(); 1];
    let first = queue
        .wait(&mut output, ROUND_TIMEOUT_NS)
        .map_err(|_| "wait first")?;
    if first != 1 || output[0].events & HUP == 0 {
        return Err("first not hup");
    }
    let first_events = output[0].events;
    let second = queue
        .wait(&mut output, ROUND_TIMEOUT_NS)
        .map_err(|_| "wait second")?;
    if second != 1 || output[0].events & HUP == 0 {
        return Err("second not hup");
    }
    println!(
        "readiness: hup-repoll first=0x{:x} second=0x{:x} ok",
        first_events, output[0].events
    );
    Ok(())
}

fn eintr_waiter(_: usize) {
    let queue = EINTR_QUEUE.load(Ordering::Acquire);
    let mut output = [EvqEvent::default(); 1];
    let mut saw = 0u32;
    for _ in 0..10 {
        // SAFETY: EVQ_WAIT reads one output record from the live stack array and validates the
        // queue handle; the timeout is a scalar.
        let raw = unsafe {
            sys::syscall4(
                Syscall::EVQ_WAIT as usize,
                queue as usize,
                output.as_mut_ptr() as usize,
                1,
                EINTR_TIMEOUT_NS as usize,
            )
        };
        if raw == Errno::EINTR.to_ret() {
            saw = saw.saturating_add(1);
            EINTR_SAW.store(saw, Ordering::Release);
            continue;
        }
        if raw == 1 && output[0].cookie == 3 {
            EINTR_DONE.store(1, Ordering::Release);
            return;
        }
        EINTR_DONE.store(-1, Ordering::Release);
        return;
    }
    EINTR_DONE.store(-2, Ordering::Release);
}

fn eintr_reevaluate() -> R {
    let queue = EventQueue::create().map_err(|_| "queue")?;
    let event = event_create().map_err(|_| "event")?;
    queue.add(event, SIGNALED, 3).map_err(|_| "add")?;
    EINTR_QUEUE.store(queue.handle().0, Ordering::Release);
    EINTR_EVENT.store(event.0, Ordering::Release);
    let worker = thread::spawn(eintr_waiter, 0, STACK_SIZE).map_err(|_| "thread")?;

    nanosleep(KICK_DELAY_NS).map_err(|_| "sleep")?;
    let thread_handle = worker.handle().ok_or("no thread handle")?;
    // SAFETY: THREAD_KICK takes the thread handle and a scalar bit mask; the handle came from
    // THREAD_CREATE and carries SIGNAL.
    let kicked = unsafe { sys::syscall2(THREAD_KICK, thread_handle.0 as usize, KICK_BIT) };
    if kicked < 0 {
        return Err("kick");
    }
    event_signal(event, 1).map_err(|_| "signal")?;
    worker.join().map_err(|_| "join")?;

    let saw = EINTR_SAW.load(Ordering::Acquire);
    let done = EINTR_DONE.load(Ordering::Acquire);
    if saw == 0 || done != 1 {
        return Err("eintr");
    }
    println!("readiness: eintr retries={} result=1 ok", saw);
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("readiness: start");
    let mut failed = 0;
    for (name, row) in [
        ("lost-wake", lost_wake as fn() -> R),
        ("hup-repoll", hup_repoll),
        ("eintr", eintr_reevaluate),
    ] {
        if let Err(reason) = row() {
            println!("readiness: failed {} {}", name, reason);
            failed += 1;
        }
    }
    if failed != 0 {
        return 1;
    }
    println!("readiness: done");
    0
}
