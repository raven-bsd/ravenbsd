// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S85a SMP lock/time/IPI stress fixture.
//!
//! Eight user workers pass a futex token around a ring on four CPUs, so every handoff is a
//! cross-CPU wake through the kernel turnstile path. Four per-CPU timer threads sleep on a 1 ms
//! interval and record the wake jitter, exercising the per-CPU cyclic timers and the timer IPI.
//! The controller runs 50 iterations of 100 ms, records wake-to-run and timer-jitter p50/p90/p99/
//! max per iteration, and prints the worst values at the end. Every iteration must make progress;
//! a missing wake or a lost timer fails the run instead of being retried.
//!
//! The qtest case sends `"<iterations> <cpus> <on|off>"` after `lock-time: ready`. The suite is
//! documented in `tests/smp/lock-time/README.md`.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_SELF};
use raven_abi::types::{Handle, clock, prot};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_rt::sys;

entry!(main);

const WORKERS: usize = 8;
const TIMERS: usize = 4;
const STACK_SIZE: usize = 32 * 1024;
const MAX_SAMPLES: usize = 4096;
const MAX_TIMER_SAMPLES: usize = 4096;
const TIMER_INTERVAL_NS: u64 = 1_000_000;
const ITER_NS: u64 = 100_000_000;
const DEFAULT_ITERATIONS: usize = 50;
const DEFAULT_CPUS: usize = 4;
const DEFAULT_TIMERS: usize = 4;
const DEFAULT_MEM_PERIOD: u64 = 16;
const YIELD_PERIOD: u64 = 64;
/// Marker exit codes the kernel's `s36-lockcheck` diagnostic samples frame statistics at.
const LEAK_BASE_EXIT: i32 = 90;
const LEAK_SAMPLE_EXIT: i32 = 91;
const LEAK_WAKES: usize = 5_000;
const LEAK_TIMEOUTS: usize = 1_000;
const LEAK_DELAY_NS: u64 = 1_000_000;
const LEAK_TIMEOUT_NS: u64 = 1_000_000;
/// A wake-to-run latency above this is a lost wake or a stuck CPU.
const STALL_NS: u64 = 2_000_000_000;
/// A timer jitter above this is a lost or badly delayed per-CPU timer.
const TIMER_STALL_NS: u64 = 1_000_000_000;

static GATE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static COUNT: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
static TID: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static WAKE_T0: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
static LATENCY: [AtomicU64; MAX_SAMPLES] = [const { AtomicU64::new(0) }; MAX_SAMPLES];
static LAT_COUNT: AtomicUsize = AtomicUsize::new(0);
static TIMER_COUNT: [AtomicUsize; TIMERS] = [const { AtomicUsize::new(0) }; TIMERS];
static TIMER_JITTER: [AtomicU64; TIMERS * MAX_TIMER_SAMPLES] =
    [const { AtomicU64::new(0) }; TIMERS * MAX_TIMER_SAMPLES];
static TIMER_TID: [AtomicU32; TIMERS] = [const { AtomicU32::new(0) }; TIMERS];
static HOPS: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
static MEM_PERIOD: AtomicU64 = AtomicU64::new(DEFAULT_MEM_PERIOD);
static PIPE_R: AtomicU32 = AtomicU32::new(u32::MAX);
static PIPE_W: AtomicU32 = AtomicU32::new(u32::MAX);
static LEAK_FUTEX: AtomicU32 = AtomicU32::new(0);
static LEAK_TIMEOUT: AtomicU32 = AtomicU32::new(0);
static LEAK_READY: AtomicU32 = AtomicU32::new(0);
static LEAK_DONE: AtomicU32 = AtomicU32::new(0);
static LEAK_STOP: AtomicBool = AtomicBool::new(false);

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_SIZE]);

// Writable `.bss`: an immutable `static` would land in `.rodata` and `THREAD_CREATE` would reject
// it as a non-writable stack.
static mut STACKS: [Stack; WORKERS + TIMERS] = [const { Stack([0; STACK_SIZE]) }; WORKERS + TIMERS];

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
    // SAFETY: `index < WORKERS + TIMERS` and the array lives for the whole test.
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

fn record_timer(timer: usize, jitter: u64) {
    if timer >= TIMERS {
        return;
    }
    let index = TIMER_COUNT[timer].fetch_add(1, Ordering::AcqRel);
    if index < MAX_TIMER_SAMPLES {
        let flat = timer
            .saturating_mul(MAX_TIMER_SAMPLES)
            .saturating_add(index);
        if let Some(slot) = TIMER_JITTER.get(flat) {
            slot.store(jitter, Ordering::Release);
        }
    }
}

fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = -1i32;
    // SAFETY: PROC_WAIT takes a scalar handle and a writable `i32` pointer owned by this frame;
    // the kernel validates the handle and writes only on success.
    let raw = unsafe {
        sys::syscall2(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            &mut code as *mut i32 as usize,
        )
    };
    Errno::from_ret(raw)?;
    Ok(code)
}

fn leak_marker(initfs: &Initfs<'_>, code: i32) -> Result<i32, Errno> {
    let text = if code == LEAK_BASE_EXIT { "90" } else { "91" };
    let args = [text];
    let options = SpawnOptions {
        args: &args,
        env: &[],
    };
    let child = spawn(initfs, "/init", &options).map_err(|_| Errno::EINVAL)?;
    let status = proc_wait(child)?;
    let _ = sys::handle_close(child);
    Ok(status)
}

extern "C" fn leak_waiter(_: usize) -> ! {
    let mut seq = 0u32;
    while !LEAK_STOP.load(Ordering::Acquire) {
        seq = seq.wrapping_add(1);
        LEAK_READY.store(seq, Ordering::Release);
        let value = LEAK_FUTEX.load(Ordering::Acquire);
        let _ = sys::futex_wait(&LEAK_FUTEX, value, None);
        LEAK_DONE.store(seq, Ordering::Release);
    }
    sys::thread_exit(0)
}

fn leak_churn() -> Result<(), &'static str> {
    let worker = sys::thread_create(
        Handle::SELF_PROC,
        leak_waiter as *const () as usize,
        stack_top(0),
        0,
    )
    .map_err(|_| "leak worker")?;
    let mut cycle = 0u32;
    while (cycle as usize) < LEAK_WAKES {
        cycle = cycle.wrapping_add(1);
        let deadline = now_ns().saturating_add(1_000_000_000);
        while LEAK_READY.load(Ordering::Acquire) < cycle {
            if now_ns() > deadline {
                return Err("leak ready");
            }
            sys::yield_now();
        }
        let _ = sys::nanosleep(LEAK_DELAY_NS);
        LEAK_FUTEX.fetch_add(1, Ordering::AcqRel);
        let _ = sys::futex_wake(&LEAK_FUTEX, 1);
        let deadline = now_ns().saturating_add(1_000_000_000);
        while LEAK_DONE.load(Ordering::Acquire) < cycle {
            if now_ns() > deadline {
                return Err("leak done");
            }
            sys::yield_now();
        }
    }
    LEAK_STOP.store(true, Ordering::Release);
    LEAK_FUTEX.fetch_add(1, Ordering::AcqRel);
    let _ = sys::futex_wake(&LEAK_FUTEX, 1);
    let _ = sys::nanosleep(LEAK_DELAY_NS);
    let _ = sys::handle_close(worker);

    for _ in 0..LEAK_TIMEOUTS {
        match sys::futex_wait(&LEAK_TIMEOUT, 0, Some(LEAK_TIMEOUT_NS)) {
            Err(Errno::ETIMEDOUT) | Err(Errno::EAGAIN) => {}
            Err(_) => return Err("leak timeout errno"),
            Ok(()) => return Err("leak timeout wake"),
        }
    }
    Ok(())
}

fn leak_mode() -> i32 {
    let image = match sys::map_initfs() {
        Ok(image) => image,
        Err(_) => return fail_code("leak initfs"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => return fail_code("leak initfs parse"),
    };
    if leak_marker(&initfs, LEAK_BASE_EXIT) != Ok(LEAK_BASE_EXIT) {
        return fail_code("leak base marker");
    }
    if leak_churn().is_err() {
        return fail_code("leak churn");
    }
    println!(
        "lock-time: leak churn wake={} timeout={}",
        LEAK_WAKES, LEAK_TIMEOUTS
    );
    if leak_marker(&initfs, LEAK_SAMPLE_EXIT) != Ok(LEAK_SAMPLE_EXIT) {
        return fail_code("leak sample marker");
    }
    // Let the marker's exit diagnostic reach the serial line before the final row.
    let _ = sys::nanosleep(100_000_000);
    println!("lock-time: leak done");
    0
}

fn fail_code(message: &str) -> i32 {
    println!("lock-time: failed {message}");
    1
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
        let mem_period = MEM_PERIOD.load(Ordering::Acquire);
        if mem_period != 0 && count.checked_rem(mem_period) == Some(0) {
            memory_pressure(index);
        }
        if count.checked_rem(YIELD_PERIOD) == Some(0) {
            // Force a voluntary context switch on the current CPU; together with eight workers on
            // four CPUs this keeps the preemption path hot.
            sys::yield_now();
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

extern "C" fn timer_thread(timer: usize) -> ! {
    TIMER_TID[timer].store(thread_self_tid(), Ordering::Release);
    loop {
        if STOP.load(Ordering::Acquire) {
            break;
        }
        let t0 = now_ns();
        let _ = sys::nanosleep(TIMER_INTERVAL_NS);
        let t1 = now_ns();
        let elapsed = t1.saturating_sub(t0);
        let jitter = elapsed.saturating_sub(TIMER_INTERVAL_NS);
        record_timer(timer, jitter);
    }
    sys::thread_exit(0)
}

fn thread_create(
    stack: usize,
    entry: extern "C" fn(usize) -> !,
    argument: usize,
) -> Result<(), Errno> {
    let thread = sys::thread_create(
        Handle::SELF_PROC,
        entry as *const () as usize,
        stack_top(stack),
        argument,
    )?;
    let _ = thread;
    Ok(())
}

fn read_config() -> (usize, usize, bool, u64, usize, bool) {
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
    let first = words.next();
    let leak = first == Some("leak");
    let iterations = if leak {
        DEFAULT_ITERATIONS
    } else {
        first
            .and_then(|word| word.parse::<usize>().ok())
            .unwrap_or(DEFAULT_ITERATIONS)
    };
    let cpus = words
        .next()
        .and_then(|word| word.parse::<usize>().ok())
        .unwrap_or(DEFAULT_CPUS);
    let mode = words.next().unwrap_or("on");
    let mem_period = words
        .next()
        .and_then(|word| word.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MEM_PERIOD);
    let timers = words
        .next()
        .and_then(|word| word.parse::<usize>().ok())
        .unwrap_or(DEFAULT_TIMERS)
        .min(TIMERS);
    (
        iterations.max(1),
        cpus.max(1),
        mode != "off",
        mem_period,
        timers,
        leak,
    )
}

fn inferred_cpu(index: usize, cpus: usize, on: bool) -> usize {
    if !on || cpus <= 1 {
        return 0;
    }
    index.saturating_add(1).checked_rem(cpus).unwrap_or(0)
}

fn percentile(sorted: &[u64], percent: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = sorted
        .len()
        .saturating_mul(percent)
        .checked_div(100)
        .unwrap_or(0)
        .min(sorted.len().saturating_sub(1));
    sorted.get(index).copied().unwrap_or(0)
}

fn stats(values: &mut Vec<u64>) -> (u64, u64, u64, u64) {
    values.sort_unstable();
    (
        percentile(values, 50),
        percentile(values, 90),
        percentile(values, 99),
        values.last().copied().unwrap_or(0),
    )
}

fn gather_latency() -> Vec<u64> {
    let count = LAT_COUNT.load(Ordering::Acquire).min(MAX_SAMPLES);
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        if let Some(slot) = LATENCY.get(index) {
            values.push(slot.load(Ordering::Acquire));
        }
    }
    values
}

fn gather_timers(timers: usize) -> Vec<u64> {
    let mut values = Vec::with_capacity(timers.saturating_mul(MAX_TIMER_SAMPLES));
    for timer in 0..timers {
        let count = TIMER_COUNT[timer]
            .load(Ordering::Acquire)
            .min(MAX_TIMER_SAMPLES);
        for index in 0..count {
            let flat = timer
                .saturating_mul(MAX_TIMER_SAMPLES)
                .saturating_add(index);
            if let Some(slot) = TIMER_JITTER.get(flat) {
                values.push(slot.load(Ordering::Acquire));
            }
        }
    }
    values
}

fn fail(message: &str) -> ! {
    println!("lock-time: failed {message}");
    sys::exit(1)
}

fn marker_exit_code(args: &raven_rt::Args) -> Option<i32> {
    let code = args.get(0)?.parse::<i32>().ok()?;
    (code == LEAK_BASE_EXIT || code == LEAK_SAMPLE_EXIT).then_some(code)
}

fn main(args: raven_rt::Args) -> i32 {
    if let Some(code) = marker_exit_code(&args) {
        sys::exit(code);
    }
    println!("lock-time: ready");
    let (iterations, cpus, on, mem_period, timers, leak) = read_config();
    if leak {
        return leak_mode();
    }
    MEM_PERIOD.store(mem_period, Ordering::Release);
    println!(
        "lock-time: workers={} timers={} iterations={} cpus={} mode={} mem_period={}",
        WORKERS,
        timers,
        iterations,
        cpus,
        if on { "on" } else { "off" },
        mem_period
    );

    let (reader, writer) = match raven_rt::pipe::pipe2(raven_rt::pipe::O_NONBLOCK) {
        Ok(pipe) => pipe,
        Err(_) => fail("pipe"),
    };
    PIPE_R.store(reader.0, Ordering::Release);
    PIPE_W.store(writer.0, Ordering::Release);

    for timer in 0..timers {
        if thread_create(WORKERS + timer, timer_thread, timer).is_err() {
            fail("timer thread create");
        }
    }
    for index in 0..WORKERS {
        if thread_create(index, worker, index).is_err() {
            fail("worker thread create");
        }
    }

    // Wait for every worker and active timer to publish its tid before the ring starts.
    let deadline = now_ns().saturating_add(5_000_000_000);
    while TID.iter().any(|slot| slot.load(Ordering::Acquire) == 0)
        || TIMER_TID
            .iter()
            .take(timers)
            .any(|slot| slot.load(Ordering::Acquire) == 0)
    {
        if now_ns() > deadline {
            fail("thread start");
        }
        let _ = sys::nanosleep(1_000_000);
    }

    // Kick the ring.
    WAKE_T0[0].store(now_ns(), Ordering::Release);
    GATE[0].store(1, Ordering::Release);
    let _ = sys::futex_wake(&GATE[0], 1);

    let mut worst_p90 = 0u64;
    let mut worst_max = 0u64;
    let mut worst_timer_p90 = 0u64;
    let mut worst_timer_max = 0u64;
    for iteration in 0..iterations {
        LAT_COUNT.store(0, Ordering::Release);
        for count in TIMER_COUNT.iter() {
            count.store(0, Ordering::Release);
        }
        let start_hops = HOPS.load(Ordering::Acquire);
        let _ = sys::nanosleep(ITER_NS);
        let hops = HOPS.load(Ordering::Acquire).saturating_sub(start_hops);

        let mut latencies = gather_latency();
        let (p50, p90, p99, max) = stats(&mut latencies);
        let mut timer_values = gather_timers(timers);
        let (t50, t90, _t99, tmax) = stats(&mut timer_values);

        if latencies.is_empty() {
            fail("no latency samples");
        }
        if timers > 0 && timer_values.is_empty() {
            fail("no timer samples");
        }
        if hops == 0 {
            fail("no progress");
        }
        if max > STALL_NS {
            fail("wake latency stall");
        }
        if tmax > TIMER_STALL_NS {
            fail("timer stall");
        }

        worst_p90 = worst_p90.max(p90);
        worst_max = worst_max.max(max);
        worst_timer_p90 = worst_timer_p90.max(t90);
        worst_timer_max = worst_timer_max.max(tmax);
        println!(
            "lock-time: iter={} hops={} p50_ns={} p90_ns={} p99_ns={} max_ns={} timer_p50_ns={} timer_p90_ns={} timer_max_ns={} samples={} timer_samples={}",
            iteration,
            hops,
            p50,
            p90,
            p99,
            max,
            t50,
            t90,
            tmax,
            latencies.len(),
            timer_values.len()
        );
    }

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
            "lock-time: worker {} tid={} count={} cpu~{}",
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
        "lock-time: per-cpu cpu0={} cpu1={} cpu2={} cpu3={}",
        per_cpu.first().copied().unwrap_or(0),
        per_cpu.get(1).copied().unwrap_or(0),
        per_cpu.get(2).copied().unwrap_or(0),
        per_cpu.get(3).copied().unwrap_or(0)
    );
    println!(
        "lock-time: summary iterations={} hops={} worst_p90_ns={} worst_max_ns={} worst_timer_p90_ns={} worst_timer_max_ns={} mem_period={}",
        iterations, hops, worst_p90, worst_max, worst_timer_p90, worst_timer_max, mem_period
    );
    if DONE.load(Ordering::Acquire) != WORKERS {
        fail("completion count");
    }
    println!(
        "lock-time: complete iterations={} workers={} timers={}",
        iterations, WORKERS, timers
    );
    println!("lock-time: done");
    0
}
