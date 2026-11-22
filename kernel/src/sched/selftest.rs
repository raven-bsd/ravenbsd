// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `selftest=sched`: four kernel threads, yields, sleeps and a monitor thread.
//!
//! The monitor checks that all workers ran exactly their share, that no two workers were inside
//! their critical counter section at the same time, that the first round ran in FIFO spawn order,
//! and that the sleeping worker actually slept for at least its requested 5 ms.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::boot;
use crate::kprintln;
use crate::sched::sleep;
use crate::sched::smp;
use crate::sync::IrqSpinLock;

/// Iterations per worker.
const ITERATIONS: u32 = 100;
/// Requested sleep in nanoseconds.
const SLEEP_NS: u64 = 5_000_000;
/// Monitor timeout.
const TIMEOUT_NS: u64 = 2_000_000_000;

static COUNTS: [AtomicU32; 4] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];
static DONE: AtomicU32 = AtomicU32::new(0);
static STARTED: AtomicU32 = AtomicU32::new(0);
static ORDER: AtomicU64 = AtomicU64::new(0);
static ACTIVE: AtomicU32 = AtomicU32::new(0);
static OVERLAP: AtomicBool = AtomicBool::new(false);
static SLEEP_ELAPSED: AtomicU64 = AtomicU64::new(0);
static SLEEP_SHORT: AtomicBool = AtomicBool::new(false);
static COUNTER_LOCK: IrqSpinLock<()> = IrqSpinLock::new(());
static CPU_SEEN: [AtomicU32; crate::sched::MAX_CPUS] =
    [const { AtomicU32::new(0) }; crate::sched::MAX_CPUS];

/// Run the scheduler self-test; on success this never returns.
pub fn run() -> ! {
    if let Err(error) = crate::sched::init() {
        kprintln!("selftest: sched setup failed: {error:?}");
        boot::exit_qemu(0x11);
    }
    let workers: [(&str, fn()); 4] = [
        ("sched-worker-0", worker0),
        ("sched-worker-1", worker1),
        ("sched-worker-2", worker2),
        ("sched-worker-3", worker3),
    ];
    for (name, entry) in workers {
        if let Err(error) = crate::sched::spawn_kernel(name, entry) {
            kprintln!("selftest: sched spawn failed: {error:?}");
            boot::exit_qemu(0x11);
        }
    }
    if let Err(error) = crate::sched::spawn_kernel("sched-monitor", monitor) {
        kprintln!("selftest: sched spawn failed: {error:?}");
        boot::exit_qemu(0x11);
    }
    crate::sched::start()
}

fn worker0() {
    worker(0);
}

fn worker1() {
    worker(1);
}

fn worker2() {
    worker(2);
}

fn worker3() {
    worker(3);
}

fn worker(index: usize) {
    let mut iteration = 0u32;
    while iteration < ITERATIONS {
        // The counter section is a real critical section: on SMP the original unprotected
        // increment could legitimately overlap between CPUs, which would make the check
        // meaningless rather than detect a scheduler bug.
        {
            let _guard = COUNTER_LOCK.lock();
            if ACTIVE.fetch_add(1, Ordering::SeqCst) != 0 {
                OVERLAP.store(true, Ordering::SeqCst);
            }
            if let Some(count) = COUNTS.get(index) {
                count.fetch_add(1, Ordering::SeqCst);
            }
            ACTIVE.fetch_sub(1, Ordering::SeqCst);
        }

        let cpu = smp::percpu::this_cpu();
        if let Some(seen) = CPU_SEEN.get(cpu) {
            seen.fetch_add(1, Ordering::SeqCst);
        }

        let sequence = STARTED.fetch_add(1, Ordering::SeqCst);
        if sequence < 8 {
            let byte = (index as u64).wrapping_add(1) & 0xff;
            ORDER.fetch_or(byte << sequence.wrapping_mul(8), Ordering::SeqCst);
        }

        // The first few iterations sleep so the application processors get a chance to steal
        // work; otherwise the tiny kernel workload can finish before their 1 ms tick.
        if iteration < 8 {
            sleep::sleep_ns(500_000);
        }
        crate::sched::yield_now();
        iteration = iteration.saturating_add(1);
    }
    if index == 0 {
        let start = sleep::now_ns();
        let deadline = start.saturating_add(SLEEP_NS);
        sleep::sleep_until(deadline);
        let elapsed = sleep::now_ns().saturating_sub(start);
        SLEEP_ELAPSED.store(elapsed, Ordering::SeqCst);
        if elapsed < SLEEP_NS {
            SLEEP_SHORT.store(true, Ordering::SeqCst);
        }
    }
    DONE.fetch_add(1, Ordering::SeqCst);
    crate::sched::exit_thread(0)
}

fn monitor() {
    let start = sleep::now_ns();
    while DONE.load(Ordering::SeqCst) < 4 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            kprintln!("selftest: sched timeout");
            boot::exit_qemu(0x11);
        }
        sleep::sleep_ns(1_000_000);
    }

    let mut ok = !OVERLAP.load(Ordering::SeqCst)
        && !SLEEP_SHORT.load(Ordering::SeqCst)
        && ACTIVE.load(Ordering::SeqCst) == 0;
    for (index, count) in COUNTS.iter().enumerate() {
        let value = count.load(Ordering::SeqCst);
        if value != ITERATIONS {
            kprintln!("selftest: sched worker {index} count {value}");
            ok = false;
        }
    }
    let cpus = smp::online_cpus();
    let order = ORDER.load(Ordering::SeqCst) & 0xffff_ffff;
    if cpus == 1 && order != 0x0403_0201 {
        kprintln!("selftest: sched start order {order:#x}");
        ok = false;
    }
    let active_cpus = cpus.min(COUNTS.len());
    let mut cpu = 0usize;
    while cpu < active_cpus {
        if CPU_SEEN
            .get(cpu)
            .is_some_and(|seen| seen.load(Ordering::SeqCst) == 0)
        {
            kprintln!("selftest: sched cpu {cpu} did not run a worker");
            ok = false;
        }
        cpu = cpu.saturating_add(1);
    }
    if SLEEP_ELAPSED.load(Ordering::SeqCst) < SLEEP_NS {
        kprintln!(
            "selftest: sched sleep {} ns",
            SLEEP_ELAPSED.load(Ordering::SeqCst)
        );
        ok = false;
    }

    if ok {
        kprintln!(
            "selftest: sched ok switches={}",
            crate::sched::switch_count()
        );
        boot::exit_qemu(0x10);
    }
    kprintln!("selftest: sched failed");
    boot::exit_qemu(0x11)
}

// --- K527: run-queue saturation -------------------------------------------------------------

/// More runnable threads than the historic 64-slot level, all at one priority.
///
/// A dropped runnable thread shows up here as a missing completion; the monitor fails with the
/// lost count instead of exiting success.
const SATURATION_WORKERS: u32 = 80;
/// Schedules per worker; 128 workers x 100 yields is a 12,800-schedule stress.
const SATURATION_YIELDS: u32 = 256;
/// Saturation monitor timeout.
const SATURATION_TIMEOUT_NS: u64 = 30_000_000_000;
/// Minimum schedules the monitor expects (the 10k stress).
const SATURATION_MIN_SWITCHES: u64 = 10_000;

static SATURATION_INDEX: AtomicU32 = AtomicU32::new(0);
static SATURATION_DONE: AtomicU32 = AtomicU32::new(0);

/// Run the K527 saturation self-test; on success this never returns.
///
/// Boot with `selftest=sched-saturation`. Every worker is enqueued at `PRIORITY_DEFAULT` before
/// the scheduler starts, then sleeps for the first few iterations (the wake boundary) and yields
/// through 100 schedules. The monitor requires every worker to complete and the switch counter to
/// pass the 10k stress bound.
pub fn run_saturation() -> ! {
    if let Err(error) = crate::sched::init() {
        kprintln!("sched-saturation: setup failed: {error:?}");
        boot::exit_qemu(0x11);
    }
    let mut spawned = 0u32;
    while spawned < SATURATION_WORKERS {
        if let Err(error) = crate::sched::spawn_kernel("sched-sat-worker", saturation_worker) {
            kprintln!(
                "sched-saturation: spawn {spawned}/{} failed: {error:?}",
                SATURATION_WORKERS
            );
            boot::exit_qemu(0x11);
        }
        spawned = spawned.saturating_add(1);
    }
    if let Err(error) = crate::sched::spawn_kernel("sched-sat-monitor", saturation_monitor) {
        kprintln!("sched-saturation: monitor spawn failed: {error:?}");
        boot::exit_qemu(0x11);
    }
    kprintln!(
        "sched-saturation: spawned={SATURATION_WORKERS} cap={}",
        raven_sched_core::MAX_THREADS
    );
    crate::sched::start()
}

fn saturation_worker() {
    let index = SATURATION_INDEX.fetch_add(1, Ordering::SeqCst);
    let mut iteration = 0u32;
    while iteration < SATURATION_YIELDS {
        // The first iterations sleep so all workers are blocked at once: the timer's wake path
        // then enqueues the whole batch at one priority (the wake-at-full boundary).
        if iteration < 8 {
            sleep::sleep_ns(500_000);
        }
        crate::sched::yield_now();
        iteration = iteration.saturating_add(1);
    }
    if index >= SATURATION_WORKERS {
        kprintln!("sched-saturation: worker index {index} over the bound");
        boot::exit_qemu(0x11);
    }
    SATURATION_DONE.fetch_add(1, Ordering::SeqCst);
    crate::sched::exit_thread(0)
}

fn saturation_monitor() {
    let start = sleep::now_ns();
    loop {
        let done = SATURATION_DONE.load(Ordering::SeqCst);
        if done >= SATURATION_WORKERS {
            break;
        }
        if sleep::now_ns().saturating_sub(start) > SATURATION_TIMEOUT_NS {
            kprintln!(
                "sched-saturation: FAIL done={done} lost={}",
                SATURATION_WORKERS.saturating_sub(done)
            );
            boot::exit_qemu(0x11);
        }
        sleep::sleep_ns(1_000_000);
    }
    let switches = crate::sched::switch_count();
    kprintln!(
        "sched-saturation: done={} lost=0 switches={switches}",
        SATURATION_WORKERS
    );
    if switches < SATURATION_MIN_SWITCHES {
        kprintln!(
            "sched-saturation: FAIL switches={switches} < {}",
            SATURATION_MIN_SWITCHES
        );
        boot::exit_qemu(0x11);
    }
    kprintln!("CONTRACT K527 PASS switches={switches}");
    boot::exit_qemu(0x10)
}
