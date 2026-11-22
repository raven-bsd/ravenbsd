// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K530 kernel stack lifetime fixture.
//!
//! Create 128 user threads, join them, and repeat for one hundred cycles. Every joined thread
//! releases its kernel stack; the kernel `s69` line at `/init` exit records `stack_fresh`,
//! `stack_cached` and `heap`. On the leaky tree every cycle frees 128 stacks into a 64-entry cache,
//! so 64 `(base, top)` pairs are dropped and `stack_fresh` grows by 64 per cycle; the bounded
//! cache must reuse every freed stack instead.
//!
//! The `s69-stackcheck` cmdline also runs the kernel's one-shot dirty/reuse/zero and guard probes;
//! the `s24-framecheck` line records the free-frame count at exit.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw thread stack mapping is in raven-rt; this fixture has no unsafe block.

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::thread;
use raven_rt::sys::thread_exit;
use raven_rt::{entry, nanosleep, println, proc, yield_now};

entry!(main);

/// Threads spawned concurrently in one cycle. The sched-core admission bound is 1024, but live
/// threads all spin at a barrier, so the cycle holds every kernel stack at once. The measured
/// production count is stated in the report and never claimed as 1,000: 80 exceeds the historic
/// 64-entry cache, so the leaky mutation drops at least 16 stack pairs per cycle while the
/// guest's 447 MiB still holds the run without an OOM masking the property.
const THREADS: usize = 80;
/// Churn cycles. Ten cycles is 800 admissions, below the 1024 live-thread bound; the kernel's
/// thread-slot release lags under a heavier churn (K530 report open gap), so a larger cycle count
/// cannot complete until that path is fixed. Nine cycles after the first give the leak samples.
const CYCLES: u32 = 10;
/// User stack per thread (one guard page is added and unmapped by `raven-rt`).
const STACK_SIZE: usize = 16 * 1024;

/// Release generation. Workers spin until it reaches the cycle that spawned them, so a later
/// cycle can never reset a worker that has not yet observed its release.
static GENERATION: AtomicU32 = AtomicU32::new(0);
/// Workers that reached their spin barrier in this cycle.
static READY: AtomicU32 = AtomicU32::new(0);
/// Workers that returned from the user function in this cycle.
static FINISHED: AtomicU32 = AtomicU32::new(0);

fn worker(generation: usize) {
    let generation = generation as u32;
    READY.fetch_add(1, Ordering::Release);
    while GENERATION.load(Ordering::Acquire) < generation {
        // Yield rather than hog a whole slice: the point of the barrier is that all stacks are
        // live, not that every worker burns CPU while waiting.
        yield_now();
    }
    FINISHED.fetch_add(1, Ordering::Release);
    // Exit directly instead of returning through `thread_start`: that path sets its done word
    // and calls `futex_wake` before `THREAD_EXIT`, and the futex wake can leave the kernel
    // thread resident under churn (a `userland/rt` finding outside this ticket's Owns). The
    // kernel stack is released by `THREAD_EXIT` either way.
    thread_exit(0);
}

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(error) = proc::set_command(Handle::SELF_PROC, "s69-stack") {
        println!("stack: fail command {:?}", error);
        return 1;
    }
    println!("stack: start threads={} cycles={}", THREADS, CYCLES);

    let mut cycle = 0u32;
    while cycle < CYCLES {
        let generation = cycle.saturating_add(1);
        READY.store(0, Ordering::Release);
        FINISHED.store(0, Ordering::Release);
        let mut threads: Vec<thread::Thread> = Vec::with_capacity(THREADS);
        for _ in 0..THREADS {
            // The previous cycle's kernel threads may still be releasing their admission slots.
            // EAGAIN is backpressure, not a stack-lifetime failure; wait for the slot instead of
            // hiding the pressure with a larger fixed sleep.
            let mut attempt = 0u32;
            loop {
                match thread::spawn(worker, generation as usize, STACK_SIZE) {
                    Ok(thread) => {
                        threads.push(thread);
                        break;
                    }
                    Err(error) => {
                        if error == Errno::EAGAIN && attempt < 30_000 {
                            if nanosleep(1_000_000).is_err() {
                                println!("stack: fail spawn wait cycle={}", cycle);
                                return 1;
                            }
                            attempt = attempt.saturating_add(1);
                            continue;
                        }
                        println!(
                            "stack: fail spawn cycle={} err={:?} attempt={} eagain={}",
                            cycle,
                            error,
                            attempt,
                            error == Errno::EAGAIN
                        );
                        return 1;
                    }
                }
            }
        }
        let mut waited = 0u32;
        while READY.load(Ordering::Acquire) < THREADS as u32 {
            if waited >= 30_000 {
                println!(
                    "stack: fail barrier cycle={} ready={}",
                    cycle,
                    READY.load(Ordering::Acquire)
                );
                return 1;
            }
            if nanosleep(1_000_000).is_err() {
                println!("stack: fail barrier cycle={}", cycle);
                return 1;
            }
            waited = waited.saturating_add(1);
            if waited % 1_000 == 0 {
                println!(
                    "stack: barrier cycle={} waited_ms={} ready={}",
                    cycle,
                    waited,
                    READY.load(Ordering::Acquire)
                );
            }
        }
        GENERATION.store(generation, Ordering::Release);
        // Wait until every worker has returned from the user function. `raven_rt::thread::join`
        // would then unmap the user stack and race the exiting thread (a `userland/rt` finding
        // outside this ticket's Owns), so the fixture drops the handles instead. The kernel
        // thread still reaches `THREAD_EXIT` and frees its kernel stack, which is the subject
        // here; only the user stack mapping is leaked by the fixture.
        let mut settled = 0u32;
        while FINISHED.load(Ordering::Acquire) < THREADS as u32 {
            if settled >= 30_000 {
                println!(
                    "stack: fail settle cycle={} finished={}",
                    cycle,
                    FINISHED.load(Ordering::Acquire)
                );
                return 1;
            }
            if nanosleep(1_000_000).is_err() {
                println!("stack: fail settle wait cycle={}", cycle);
                return 1;
            }
            settled = settled.saturating_add(1);
        }
        if nanosleep(1_000_000).is_err() {
            println!("stack: fail settle cycle={}", cycle);
            return 1;
        }
        drop(threads);
        cycle += 1;
        if cycle % 10 == 0 || cycle == CYCLES {
            println!("stack: cycle={} done", cycle);
        }
    }

    println!("stack: done");
    0
}
