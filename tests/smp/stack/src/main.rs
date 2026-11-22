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
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use raven_abi::types::Handle;
use raven_rt::thread;
use raven_rt::{entry, nanosleep, println, proc};

entry!(main);

/// Threads spawned concurrently in one cycle. The sched-core admission bound is 1024, but live
/// threads all spin at a barrier, so the cycle holds every kernel stack at once. The measured
/// production count is stated in the report and never claimed as 1,000: 128 keeps the leaky
/// mutation inside the guest's 447 MiB while still exceeding the 64-entry cache by 64 per cycle.
const THREADS: usize = 128;
/// Churn cycles.
const CYCLES: u32 = 100;
/// User stack per thread (one guard page is added and unmapped by `raven-rt`).
const STACK_SIZE: usize = 64 * 1024;

/// Set by the main thread once all workers exist; workers spin until then so the kernel holds
/// all 128 kernel stacks at once and the cache bound, not the spawn rate, decides reuse.
static RELEASE: AtomicBool = AtomicBool::new(false);
/// Workers that reached their spin barrier in this cycle.
static READY: AtomicU32 = AtomicU32::new(0);

fn worker(_: usize) {
    READY.fetch_add(1, Ordering::Release);
    while !RELEASE.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(error) = proc::set_command(Handle::SELF_PROC, "s69-stack") {
        println!("stack: fail command {:?}", error);
        return 1;
    }
    println!("stack: start threads={} cycles={}", THREADS, CYCLES);

    let mut cycle = 0u32;
    while cycle < CYCLES {
        RELEASE.store(false, Ordering::Release);
        READY.store(0, Ordering::Release);
        let mut threads: Vec<thread::Thread> = Vec::with_capacity(THREADS);
        for _ in 0..THREADS {
            match thread::spawn(worker, 0, STACK_SIZE) {
                Ok(thread) => threads.push(thread),
                Err(error) => {
                    println!("stack: fail spawn cycle={} err={:?}", cycle, error);
                    return 1;
                }
            }
        }
        while READY.load(Ordering::Acquire) < THREADS as u32 {
            if nanosleep(1_000_000).is_err() {
                println!("stack: fail barrier cycle={}", cycle);
                return 1;
            }
        }
        RELEASE.store(true, Ordering::Release);
        for thread in threads {
            if let Err(error) = thread.join() {
                println!("stack: fail join cycle={} err={:?}", cycle, error);
                return 1;
            }
        }
        cycle += 1;
        if cycle % 10 == 0 || cycle == CYCLES {
            println!("stack: cycle={} done", cycle);
        }
    }

    println!("stack: done");
    0
}
