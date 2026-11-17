// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S76c aarch64 four-core user-SMP soak.
//!
//! Eight user threads pass a futex token around a ring. Each holder runs one work unit before
//! waking the next thread:
//!
//! * FP/SIMD: the worker re-checks the private `V0` tag it stored just before blocking, then runs
//!   a deterministic two-lane vector add whose result it verifies. A scheduler that loses the
//!   per-thread `Q0..Q31`/`FPSR`/`FPCR` area on the shared CPU shows up as a wrong tag here.
//! * TLB churn: every [`MEM_PERIOD`] hops it creates a memory object, maps it shared, writes both
//!   pages, reads them back, then unmaps it (one remote shootdown per unmap) and closes the object.
//! * Pipe traffic: one byte through a shared non-blocking pipe.
//!
//! The controller measures the wake-to-run latency of every handoff, runs for the requested
//! duration, then stops the ring and prints per-thread counts, per-CPU totals (inferred from the
//! kernel's round-robin placement; aarch64 user threads do not migrate after placement), hops,
//! throughput and p50/p99 latency.
//!
//! The mode is compiled in: `bin/on.rs` runs with `smp_user=on`, `bin/off.rs` with `smp_user=off`.
//! The aarch64 kernel has no console input path, so the fixture does not read its configuration
//! from the serial line the way the x86-64 soak does.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_SELF};
use raven_abi::types::{Handle, clock, prot};
use raven_rt::println;
use raven_rt::sys;

/// Ring size. The ticket requires at least eight user threads.
pub const WORKERS: usize = 8;
/// Latency samples kept for the percentile summary.
const MAX_SAMPLES: usize = 8192;
/// One stack per worker.
const STACK_SIZE: usize = 32 * 1024;
/// Hops between two TLB-churn work units on one worker.
const MEM_PERIOD: u64 = 8;
/// Pages in one churn mapping.
const MAP_PAGES: usize = 2;
const PAGE: usize = 4096;
/// Configured CPU count for the QEMU `-smp 4` gate.
pub const CPUS: usize = 4;
/// A ring that makes no progress for this long is a lost wake or a stuck CPU.
const STALL_NS: u64 = 3_000_000_000;

static GATE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static COUNT: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
static TID: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static WAKE_T0: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
static LATENCY: [AtomicU64; MAX_SAMPLES] = [const { AtomicU64::new(0) }; MAX_SAMPLES];
static LAT_COUNT: AtomicUsize = AtomicUsize::new(0);
static HOPS: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
static FP_BAD: AtomicU64 = AtomicU64::new(0);
static MAP_BAD: AtomicU64 = AtomicU64::new(0);
static MEM_CHURN: AtomicU64 = AtomicU64::new(0);
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
    // SAFETY: only the address is taken; the memory is used as a thread stack, never as a Rust
    // value.
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

/// Per-thread FP/SIMD tag. Distinct per worker, so a lost register shows the sibling's tag.
fn fp_tag(index: usize) -> u64 {
    0x5a5a_0000_0000_0000u64 | u64::try_from(index).unwrap_or(0)
}

/// Store the tag in both lanes of `V0` before the worker blocks.
///
/// The target is `aarch64-unknown-none-softfloat`, so the compiler never allocates `V0..V31`;
/// touching them from inline asm cannot clobber a live Rust value.
fn fp_store(tag: u64) {
    // SAFETY: the `.arch` directive enables the FP/SIMD encodings this target does not assemble by
    // default; `V0` is not an ABI register of the soft-float compiler.
    unsafe {
        core::arch::asm!(
            ".arch armv8-a+fp+simd",
            "dup v0.2d, {tag}",
            tag = in(reg) tag,
            options(nostack, preserves_flags),
        );
    }
}

/// True when both lanes of `V0` still hold `tag`.
fn fp_verify(tag: u64) -> bool {
    let low: u64;
    let high: u64;
    // SAFETY: as in `fp_store`; the two moves only read `V0`.
    unsafe {
        core::arch::asm!(
            ".arch armv8-a+fp+simd",
            "fmov {low}, d0",
            "mov {high}, v0.d[1]",
            low = out(reg) low,
            high = out(reg) high,
            options(nostack, preserves_flags),
        );
    }
    low == tag && high == tag
}

/// Deterministic two-lane vector add: `[tag, tag] + [1, 2]` must equal `[tag+1, tag+2]`.
fn fp_unit(tag: u64) -> bool {
    let low: u64;
    let high: u64;
    // SAFETY: as in `fp_store`. `x9`/`x10` are declared clobbered; the vector registers are not
    // allocated by the soft-float compiler.
    unsafe {
        core::arch::asm!(
            ".arch armv8-a+fp+simd",
            "dup v1.2d, {tag}",
            "mov x9, #1",
            "mov x10, #2",
            "fmov d2, x9",
            "mov v2.d[1], x10",
            "add v3.2d, v1.2d, v2.2d",
            "fmov {low}, d3",
            "mov {high}, v3.d[1]",
            tag = in(reg) tag,
            low = out(reg) low,
            high = out(reg) high,
            out("x9") _,
            out("x10") _,
            options(nostack, preserves_flags),
        );
    }
    low == tag.wrapping_add(1) && high == tag.wrapping_add(2)
}

/// Map, touch, verify and unmap a fresh memory object. The unmap is one remote TLB shootdown.
fn tlb_churn(seed: u64) -> bool {
    let Ok(object) = memobj_create(MAP_PAGES * PAGE) else {
        return false;
    };
    let mut ok = false;
    if let Ok(address) =
        sys::map_object(object, 0, MAP_PAGES * PAGE, prot::READ | prot::WRITE, true)
    {
        let high_address = address.saturating_add(PAGE);
        let expected_high = seed ^ 0xffff_ffff_ffff_ffff;
        // SAFETY: the kernel mapped two writable pages at `address`; both offsets are inside it.
        unsafe {
            (address as *mut u64).write_volatile(seed);
            (high_address as *mut u64).write_volatile(expected_high);
        }
        // SAFETY: as above; the reads stay inside the mapping.
        let low = unsafe { (address as *const u64).read_volatile() };
        // SAFETY: as above.
        let high = unsafe { (high_address as *const u64).read_volatile() };
        ok = low == seed && high == expected_high;
        let _ = sys::unmap(Handle::SELF_PROC, address, MAP_PAGES * PAGE);
    }
    let _ = sys::handle_close(object);
    MEM_CHURN.fetch_add(1, Ordering::AcqRel);
    ok
}

fn pipe_token(seed: usize) {
    let reader = Handle(PIPE_R.load(Ordering::Acquire));
    let writer = Handle(PIPE_W.load(Ordering::Acquire));
    let mut byte = [0u8; 1];
    let _ = sys::read(reader, &mut byte);
    let _ = sys::write(writer, &[seed as u8]);
}

extern "C" fn worker(index: usize) -> ! {
    let tag = fp_tag(index);
    TID[index].store(thread_self_tid(), Ordering::Release);
    let next = index.saturating_add(1).checked_rem(WORKERS).unwrap_or(0);
    // Park the tag before the first block; every later iteration re-parks it after the wake.
    fp_store(tag);
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
        // The scheduler may have run the sibling worker on this CPU while this one blocked: the
        // tag proves the per-thread FP/SIMD area survived the switch.
        if !fp_verify(tag) {
            FP_BAD.fetch_add(1, Ordering::AcqRel);
        }
        if !fp_unit(tag) {
            FP_BAD.fetch_add(1, Ordering::AcqRel);
        }
        fp_store(tag);
        GATE[index].store(0, Ordering::Release);
        if STOP.load(Ordering::Acquire) {
            break;
        }

        let count = COUNT[index]
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        pipe_token(index);
        if count.checked_rem(MEM_PERIOD) == Some(0) && !tlb_churn(tag ^ count) {
            MAP_BAD.fetch_add(1, Ordering::AcqRel);
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

/// CPU the kernel's round-robin policy placed worker `index` on.
///
/// With `smp_user=on` the first user thread (this process's main thread) takes CPU 0 and each
/// later `THREAD_CREATE` advances the counter, so worker `index` lands on `(index + 1) % CPUS`.
/// aarch64 user threads resume on their `last_cpu` and are never stolen across CPUs, so the
/// placement is stable for the whole run. With `smp_user=off` every user thread stays on CPU 0.
fn inferred_cpu(index: usize, mode_on: bool) -> usize {
    if !mode_on || CPUS <= 1 {
        return 0;
    }
    index.saturating_add(1).checked_rem(CPUS).unwrap_or(0)
}

fn fail(message: &str) -> i32 {
    println!("smp-soak-aarch64: failed {message}");
    1
}

/// Run the soak for `duration_s` seconds. `mode_on` mirrors the `smp_user` boot flag so the
/// per-CPU inference matches the kernel's placement policy.
pub fn run(duration_s: u64, mode_on: bool) -> i32 {
    println!(
        "smp-soak-aarch64: workers={WORKERS} duration_s={duration_s} cpus={CPUS} mode={}",
        if mode_on { "on" } else { "off" }
    );

    let (reader, writer) = match raven_rt::pipe::pipe2(raven_rt::pipe::O_NONBLOCK) {
        Ok(pipe) => pipe,
        Err(_) => return fail("pipe"),
    };
    PIPE_R.store(reader.0, Ordering::Release);
    PIPE_W.store(writer.0, Ordering::Release);

    for index in 0..WORKERS {
        if thread_create(index).is_err() {
            return fail("thread create");
        }
    }

    // Wait for every worker to publish its tid before the ring starts.
    let deadline = now_ns().saturating_add(5_000_000_000);
    while TID.iter().any(|slot| slot.load(Ordering::Acquire) == 0) {
        if now_ns() > deadline {
            return fail("worker start");
        }
        let _ = sys::nanosleep(1_000_000);
    }

    let started = now_ns();
    WAKE_T0[0].store(started, Ordering::Release);
    GATE[0].store(1, Ordering::Release);
    let _ = sys::futex_wake(&GATE[0], 1);

    // Run the ring and watch for a lost wake or a stuck CPU: the hop counter must keep moving.
    let end = started.saturating_add(duration_s.saturating_mul(1_000_000_000));
    let mut last_hops = 0u64;
    let mut last_progress = started;
    while now_ns() < end {
        let _ = sys::nanosleep(100_000_000);
        let hops = HOPS.load(Ordering::Acquire);
        let now = now_ns();
        if hops != last_hops {
            last_hops = hops;
            last_progress = now;
        } else if now.saturating_sub(last_progress) > STALL_NS {
            STOP.store(true, Ordering::Release);
            return fail("ring stalled");
        }
    }

    STOP.store(true, Ordering::Release);
    for gate in GATE.iter() {
        gate.store(1, Ordering::Release);
        let _ = sys::futex_wake(gate, 1);
    }
    let deadline = now_ns().saturating_add(15_000_000_000);
    while DONE.load(Ordering::Acquire) < WORKERS {
        if now_ns() > deadline {
            return fail("workers stuck");
        }
        let _ = sys::nanosleep(1_000_000);
    }

    let mut total = 0u64;
    let mut per_cpu = [0u64; CPUS];
    for index in 0..WORKERS {
        let count = COUNT[index].load(Ordering::Acquire);
        total = total.saturating_add(count);
        let cpu = inferred_cpu(index, mode_on);
        if let Some(slot) = per_cpu.get_mut(cpu) {
            *slot = slot.saturating_add(count);
        }
        println!(
            "smp-soak-aarch64: thread {} tid={} count={} cpu~{}",
            index,
            TID[index].load(Ordering::Acquire),
            count,
            cpu
        );
    }
    let hops = HOPS.load(Ordering::Acquire);
    if total != hops {
        return fail("hop count mismatch");
    }
    if FP_BAD.load(Ordering::Acquire) != 0 {
        println!(
            "smp-soak-aarch64: fp_bad={}",
            FP_BAD.load(Ordering::Acquire)
        );
        return fail("fp/simd corruption");
    }
    if MAP_BAD.load(Ordering::Acquire) != 0 {
        println!(
            "smp-soak-aarch64: map_bad={}",
            MAP_BAD.load(Ordering::Acquire)
        );
        return fail("stale mapping");
    }

    println!(
        "smp-soak-aarch64: per-cpu cpu0={} cpu1={} cpu2={} cpu3={}",
        per_cpu.first().copied().unwrap_or(0),
        per_cpu.get(1).copied().unwrap_or(0),
        per_cpu.get(2).copied().unwrap_or(0),
        per_cpu.get(3).copied().unwrap_or(0)
    );
    let elapsed_s = now_ns()
        .saturating_sub(started)
        .checked_div(1_000_000_000)
        .unwrap_or(1);
    println!(
        "smp-soak-aarch64: hops={} total={} churn={} ops_per_s={}",
        hops,
        total,
        MEM_CHURN.load(Ordering::Acquire),
        total.checked_div(elapsed_s.max(1)).unwrap_or(0)
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
        "smp-soak-aarch64: latency p50_ns={} p99_ns={} samples={}",
        p50, p99, used
    );

    if DONE.load(Ordering::Acquire) != WORKERS {
        return fail("completion count");
    }
    println!("smp-soak-aarch64: complete workers={WORKERS}");
    println!("smp-soak-aarch64: done");
    0
}
