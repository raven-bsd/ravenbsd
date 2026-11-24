// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K530 join-lifetime fixture.
//!
//! All 64 workers are live at a barrier, released together, then joined. The pre-fix
//! `raven_rt::thread::join` observed a user-published done word and unmapped the user stack while
//! the exiting thread was still running its `futex_wake`/`THREAD_EXIT` path on that stack; this
//! fixture faults. The fixed join waits for the kernel's `THREAD_EXIT` clear word before unmapping.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // The fixture has no unsafe block; raven-rt owns the stack mapping.

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::thread;
use raven_rt::{entry, nanosleep, println, proc, yield_now};

entry!(main);

const THREADS: usize = 64;
const ROUNDS: u32 = 8;
const STACK_SIZE: usize = 16 * 1024;

static READY: AtomicU32 = AtomicU32::new(0);
static GENERATION: AtomicU32 = AtomicU32::new(0);

fn worker(generation: usize) {
    let generation = generation as u32;
    READY.fetch_add(1, Ordering::Release);
    while GENERATION.load(Ordering::Acquire) < generation {
        yield_now();
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(error) = proc::set_command(Handle::SELF_PROC, "s69-stack-join") {
        println!("stack-join: fail command {:?}", error);
        return 1;
    }
    println!("stack-join: start rounds={} threads={}", ROUNDS, THREADS);

    let mut round = 0u32;
    while round < ROUNDS {
        let generation = round.saturating_add(1);
        READY.store(0, Ordering::Release);
        let mut threads: Vec<thread::Thread> = Vec::with_capacity(THREADS);
        for _ in 0..THREADS {
            match thread::spawn(worker, generation as usize, STACK_SIZE) {
                Ok(thread) => threads.push(thread),
                Err(error) => {
                    println!("stack-join: fail spawn round={} err={:?}", round, error);
                    return 1;
                }
            }
        }
        let mut waited = 0u32;
        while READY.load(Ordering::Acquire) < THREADS as u32 {
            if waited >= 30_000 {
                println!(
                    "stack-join: fail barrier round={} ready={}",
                    round,
                    READY.load(Ordering::Acquire)
                );
                return 1;
            }
            if nanosleep(1_000_000).is_err() {
                println!("stack-join: fail barrier round={}", round);
                return 1;
            }
            waited = waited.saturating_add(1);
        }
        GENERATION.store(generation, Ordering::Release);
        for thread in threads {
            if let Err(error) = thread.join() {
                println!("stack-join: fail join round={} err={:?}", round, error);
                return 1;
            }
        }
        round = round.saturating_add(1);
    }

    println!("stack-join: done rounds={} threads={}", ROUNDS, THREADS);
    0
}
