// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S75b user-SMP soak fixture.
//!
//! Eight user threads pass a futex token around a ring. Each holder performs a work unit (counter,
//! non-blocking pipe traffic and a small memory-object map/unmap) before waking the next thread,
//! so every handoff is a cross-CPU wake when `smp_user=on`. The controller records the wake-to-run
//! latency of every handoff, runs for a configurable duration (default 60 s), stops the ring and
//! prints per-thread counts, per-CPU work (inferred from the round-robin creation policy), hops,
//! throughput and p50/p99 latency.
//!
//! Configuration: the controller prints `smp-soak: ready` and reads one console line
//! `"<seconds> <cpus> <on|off>"` (default `60 4 on`). The qtest sends that line.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_SELF};
use raven_abi::types::{Handle, clock, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const WORKERS: usize = 8;
const MAX_SAMPLES: usize = 4096;
const MEM_PERIOD: u64 = 16;
const STACK_SIZE: usize = 32 * 1024;
const DEFAULT_SECONDS: u64 = 60;
const DEFAULT_CPUS: usize = 4;

static GATE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static COUNT: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
static TID: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static WAKE_T0: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
static LATENCY: [AtomicU64; MAX_SAMPLES] = [const { AtomicU64::new(0) }; MAX_SAMPLES];
static LAT_COUNT: AtomicUsize = AtomicUsize::new(0);
static HOPS: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
static PIPE_R: AtomicU32 = AtomicU32::new(u32::MAX);
static PIPE_W: AtomicU32 = AtomicU32::new(u32::MAX);

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_SIZE]);

// Writable `.bss`: an immutable `static` would land in `.rodata` and `THREAD_CREATE` would reject
// it as a non-writable stack.
static mut STACKS: [Stack; WORKERS] = [const { Stack([0; STACK_SIZE]) }; WORKERS];

fn now_ns() -> u64 {
    match sys::clock_get(clock::MONOTONIC) {
        Ok(timespec) => u64::try_from(timespec.tv_sec)
            .unwrap_or(0)
            .saturating_mul(1_000_000_000)
            .saturating_add(u64::try_from(timespec.tv_nsec).unwrap_or(0)),
        Err(_) => 0,
    }
}

fn stack_top(index: usize) -> usize {
    // SAFETY: only the address is taken; the memory is used as a kernel/user thread stack, never
    // as a Rust value.
    let base = core::ptr::addr_of_mut!(STACKS) as *mut Stack;
    // SAFETY: `index < WORKERS` and the array lives for the whole test.
    let slot = unsafe { base.add(index) };
    (slot as usize).saturating_add(STACK_SIZE)
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes a page-aligned length and zero flags.
    let raw = unsafe { sys::syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0) };
    let value = Errno::from_ret(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn thread_self_tid() -> u32 {
    // SAFETY: THREAD_SELF takes no arguments and returns this thread's tid.
    let raw = unsafe { sys::syscall0(THREAD_SELF) };
    u32::try_from(raw).unwrap_or(0)
}

fn record_latency(ns: u64) {
    let index = LAT_COUNT.fetch_add(1, Ordering::AcqRel);
    if index < MAX_SAMPLES
        && let Some(slot) = LATENCY.get(index)
    {
        slot.store(ns, Ordering::Release);
    }
}

fn memory_pressure(seed: usize) {
    if let Ok(object) = memobj_create(4096) {
        if let Ok(address) = sys::map_object(object, 0, 4096, prot::READ | prot::WRITE, false) {
            // SAFETY: the kernel mapped a writable page at `address`.
            unsafe {
                (address as *mut u64).write_volatile(seed as u64);
            }
            let _ = sys::unmap(Handle::SELF_PROC, address, 4096);
        }
        let _ = sys::handle_close(object);
    }
}

fn pipe_token(seed: usize) {
    let reader = Handle(PIPE_R.load(Ordering::Acquire));
    let writer = Handle(PIPE_W.load(Ordering::Acquire));
    let mut byte = [0u8; 1];
    let _ = sys::read(reader, &mut byte);
    let _ = sys::write(writer, &[seed as u8]);
}

extern "C" fn worker(index: usize) -> ! {
    TID[index].store(thread_self_tid(), Ordering::Release);
    let next = index.saturating_add(1).checked_rem(WORKERS).unwrap_or(0);
    loop {
        if STOP.load(Ordering::Acquire) {
            break;
        }
        let _ = sys::futex_wait(&GATE[index], 0, None);
        let t0 = WAKE_T0[index].load(Ordering::Acquire);
        if t0 != 0 {
            let now = now_ns();
            if now > t0 {
                record_latency(now.saturating_sub(t0));
            }
        }
        GATE[index].store(0, Ordering::Release);
        if STOP.load(Ordering::Acquire) {
            break;
        }

        let count = COUNT[index]
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        pipe_token(index);
        if count.checked_rem(MEM_PERIOD) == Some(0) {
            memory_pressure(index);
        }
        HOPS.fetch_add(1, Ordering::AcqRel);
        let t0 = now_ns();
        WAKE_T0[next].store(t0, Ordering::Release);
        GATE[next].store(1, Ordering::Release);
        let _ = sys::futex_wake(&GATE[next], 1);
    }
    DONE.fetch_add(1, Ordering::AcqRel);
    sys::thread_exit(0)
}

fn thread_create(index: usize) -> Result<(), Errno> {
    let thread = sys::thread_create(
        Handle::SELF_PROC,
        worker as *const () as usize,
        stack_top(index),
        index,
    )?;
    let _ = thread;
    Ok(())
}

fn read_config() -> (u64, usize, bool) {
    let mut line = [0u8; 64];
    let mut length = 0usize;
    let deadline = now_ns().saturating_add(2_000_000_000);
    while now_ns() < deadline && length < line.len() {
        let Some(rest) = line.get_mut(length..) else {
            break;
        };
        match sys::debug_read(rest, 100_000_000) {
            Ok(0) => {}
            Ok(count) => {
                length = length.saturating_add(count);
                if line
                    .get(..length)
                    .is_some_and(|bytes| bytes.contains(&b'\n'))
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = line
        .get(..length)
        .and_then(|bytes| core::str::from_utf8(bytes).ok())
        .unwrap_or("");
    let mut words = text.split_ascii_whitespace();
    let seconds = words
        .next()
        .and_then(|word| word.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SECONDS);
    let cpus = words
        .next()
        .and_then(|word| word.parse::<usize>().ok())
        .unwrap_or(DEFAULT_CPUS);
    let mode = words.next().unwrap_or("on");
    (seconds.max(1), cpus.max(1), mode != "off")
}

fn inferred_cpu(index: usize, cpus: usize, on: bool) -> usize {
    if !on || cpus <= 1 {
        return 0;
    }
    index.saturating_add(1).checked_rem(cpus).unwrap_or(0)
}

fn fail(message: &str) -> ! {
    println!("smp-soak: failed {message}");
    sys::exit(1)
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("smp-soak: ready");
    let (seconds, cpus, on) = read_config();
    println!(
        "smp-soak: workers={} duration_s={} cpus={} mode={}",
        WORKERS,
        seconds,
        cpus,
        if on { "on" } else { "off" }
    );

    let (reader, writer) = match raven_rt::pipe::pipe2(raven_rt::pipe::O_NONBLOCK) {
        Ok(pipe) => pipe,
        Err(_) => fail("pipe"),
    };
    PIPE_R.store(reader.0, Ordering::Release);
    PIPE_W.store(writer.0, Ordering::Release);

    for index in 0..WORKERS {
        if thread_create(index).is_err() {
            fail("thread create");
        }
    }

    // Wait for every worker to publish its tid before the ring starts.
    let deadline = now_ns().saturating_add(5_000_000_000);
    while TID.iter().any(|slot| slot.load(Ordering::Acquire) == 0) {
        if now_ns() > deadline {
            fail("worker start");
        }
        let _ = sys::nanosleep(1_000_000);
    }

    // Kick the ring.
    WAKE_T0[0].store(now_ns(), Ordering::Release);
    GATE[0].store(1, Ordering::Release);
    let _ = sys::futex_wake(&GATE[0], 1);

    let _ = sys::nanosleep(seconds.saturating_mul(1_000_000_000));

    STOP.store(true, Ordering::Release);
    for gate in GATE.iter() {
        gate.store(1, Ordering::Release);
        let _ = sys::futex_wake(gate, 1);
    }
    let deadline = now_ns().saturating_add(10_000_000_000);
    while DONE.load(Ordering::Acquire) < WORKERS {
        if now_ns() > deadline {
            fail("workers stuck");
        }
        let _ = sys::nanosleep(1_000_000);
    }

    let mut total = 0u64;
    let mut per_cpu = [0u64; WORKERS];
    for index in 0..WORKERS {
        let count = COUNT[index].load(Ordering::Acquire);
        total = total.saturating_add(count);
        let cpu = inferred_cpu(index, cpus, on);
        if let Some(slot) = per_cpu.get_mut(cpu) {
            *slot = slot.saturating_add(count);
        }
        println!(
            "smp-soak: thread {} tid={} count={} cpu~{}",
            index,
            TID[index].load(Ordering::Acquire),
            count,
            cpu
        );
    }
    let hops = HOPS.load(Ordering::Acquire);
    if total != hops {
        fail("hop count mismatch");
    }

    println!(
        "smp-soak: per-cpu cpu0={} cpu1={} cpu2={} cpu3={}",
        per_cpu.first().copied().unwrap_or(0),
        per_cpu.get(1).copied().unwrap_or(0),
        per_cpu.get(2).copied().unwrap_or(0),
        per_cpu.get(3).copied().unwrap_or(0)
    );
    println!(
        "smp-soak: hops={} total={} ops_per_s={}",
        hops,
        total,
        total.checked_div(seconds).unwrap_or(0)
    );

    let samples = LAT_COUNT.load(Ordering::Acquire);
    let used = samples.min(MAX_SAMPLES);
    let mut latencies: Vec<u64> = Vec::with_capacity(used);
    for index in 0..used {
        if let Some(slot) = LATENCY.get(index) {
            latencies.push(slot.load(Ordering::Acquire));
        }
    }
    latencies.sort_unstable();
    let p50 = latencies.get(used / 2).copied().unwrap_or(0);
    let p99 = latencies
        .get(
            used.saturating_mul(99)
                .checked_div(100)
                .unwrap_or(0)
                .min(used.saturating_sub(1)),
        )
        .copied()
        .unwrap_or(0);
    println!(
        "smp-soak: latency p50_ns={} p99_ns={} samples={}",
        p50, p99, used
    );

    if DONE.load(Ordering::Acquire) != WORKERS {
        fail("completion count");
    }
    println!("smp-soak: complete workers={}", WORKERS);
    println!("smp-soak: done");
    0
}
