// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel clocks and the entropy pool.
//!
//! The monotonic clock counts from the architecture counter and the timer tick counter; the wall
//! clock (`CLOCK_REALTIME`) is the platform RTC read lazily on first use plus the monotonic
//! elapsed time, and the entropy pool feeds `GETRANDOM`/`AT_RANDOM`. The RTC and the pool are
//! wired here because the platform RTC lives in [`rtc`] and the pool needs the monotonic counter
//! for its jitter fallback.
//!
//! x86-64 uses the TSC with a calibrated scale; aarch64 uses `CNTPCT_EL0` with the frequency from
//! `CNTFRQ_EL0` (or `BootInfo.counter_freq`). Both expose the same interface to the scheduler and
//! the self-tests.

#[cfg(target_arch = "x86_64")]
use core::sync::atomic::AtomicU32;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

#[cfg(target_arch = "x86_64")]
use raven_timemath::{self, Scale};

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::timer as arch_timer;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::{apic, tsc};
use crate::sync::IrqSpinLock;
use crate::{boot, kprintln};

/// The pure per-CPU deadline timer core (`raven-cyclic`, L2), compiled from the same source the
/// host tests run. `kernel/Cargo.toml` is not in this ticket's Owns, so it is included by
/// `#[path]` (the S26/S27 pattern).
#[path = "../../crates/raven-cyclic/src/lib.rs"]
#[allow(unused_attributes)] // `raven-cyclic`'s `#![no_std]` is its own crate-root attribute, unused as a module here.
#[allow(dead_code)]
// The kernel uses `add`/`on_interrupt`/`cancel`; the rest of raven-cyclic's surface awaits R-TIM-9 consumers.
mod cyclic;

#[cfg(target_arch = "x86_64")]
#[path = "arch/x86_64/rtc.rs"]
mod rtc;
#[cfg(target_arch = "aarch64")]
#[path = "arch/aarch64/rtc.rs"]
mod rtc;

/// The pure entropy pool, compiled from the same source as the host-tested `raven-entropy` crate.
#[path = "../../crates/raven-entropy/src/lib.rs"]
mod raven_entropy;

#[cfg(target_arch = "x86_64")]
static TSC_HZ: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "x86_64")]
static BOOT_TSC: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "x86_64")]
static TSC_MULT: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "x86_64")]
static TSC_SHIFT: AtomicU32 = AtomicU32::new(0);

/// Counter frequency in Hz (aarch64).
#[cfg(target_arch = "aarch64")]
static COUNTER_HZ: AtomicU64 = AtomicU64::new(0);
/// Counter value at boot (aarch64).
#[cfg(target_arch = "aarch64")]
static BOOT_COUNTER: AtomicU64 = AtomicU64::new(0);

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Wall-clock nanoseconds at boot (zero until the RTC is read).
static BOOT_REALTIME_NS: AtomicU64 = AtomicU64::new(0);
/// Whether the RTC has been read (successfully or not).
static REALTIME_READ: AtomicBool = AtomicBool::new(false);

/// Initialise the monotonic clock.
///
/// x86-64 passes the TSC frequency and the TSC value at boot; aarch64 passes the generic-timer
/// frequency and the `CNTPCT_EL0` value at boot.
pub fn init(counter_hz: u64, boot_counter: u64) {
    #[cfg(target_arch = "x86_64")]
    {
        TSC_HZ.store(counter_hz, Ordering::Release);
        BOOT_TSC.store(boot_counter, Ordering::Release);
        if let Some(scale) = raven_timemath::scale_for_tsc(counter_hz) {
            TSC_MULT.store(scale.mult, Ordering::Release);
            TSC_SHIFT.store(scale.shift, Ordering::Release);
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        COUNTER_HZ.store(counter_hz, Ordering::Release);
        BOOT_COUNTER.store(boot_counter, Ordering::Release);
    }
    cyclic_selftest();
    entropy_init();
}

/// Nanoseconds since [`init`].
#[cfg(target_arch = "x86_64")]
pub fn now_ns() -> u64 {
    let mult = TSC_MULT.load(Ordering::Acquire);
    let shift = TSC_SHIFT.load(Ordering::Acquire);
    if mult == 0 {
        return 0;
    }
    let ticks = tsc::read().saturating_sub(BOOT_TSC.load(Ordering::Acquire));
    raven_timemath::tsc_to_ns(ticks, Scale { mult, shift })
}

/// Nanoseconds since [`init`].
#[cfg(target_arch = "aarch64")]
pub fn now_ns() -> u64 {
    let frequency = COUNTER_HZ.load(Ordering::Acquire);
    if frequency == 0 {
        return 0;
    }
    let counter = arch_timer::read_counter().saturating_sub(BOOT_COUNTER.load(Ordering::Acquire));
    // 64-bit nanoseconds overflow after a few centuries; the u128 product removes the intermediate
    // overflow at counter frequencies above ~18 GHz. Checked operations keep the kernel's
    // arithmetic policy: on overflow the clock saturates instead of wrapping.
    let nanoseconds = u128::from(counter)
        .checked_mul(1_000_000_000u128)
        .and_then(|value| value.checked_div(u128::from(frequency)))
        .unwrap_or(u128::from(u64::MAX));
    u64::try_from(nanoseconds).unwrap_or(u64::MAX)
}

/// Timer ticks since boot.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Acquire)
}

/// Called from the timer interrupt.
pub fn on_tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    cyclic_fire();
    #[cfg(target_arch = "x86_64")]
    apic::eoi();
    // S27 fallback: drain COM1 from the tick when the IOAPIC line is unavailable.
    #[cfg(target_arch = "x86_64")]
    crate::console::poll();
    crate::event::notify_timers(crate::time::now_ns());
}

// ---------------------------------------------------------------------------
// Cyclic timer integration (L4)
// ---------------------------------------------------------------------------

/// Called from an application processor's timer interrupt.
///
/// DragonFly runs a softclock per CPU, so each CPU drains its own callout wheel. Raven keeps the
/// global tick counter, wall clock, console poll and event-timer notification on the BSP; an AP
/// only drains its local cyclic wheel here.
pub fn on_ap_tick() {
    cyclic_fire();
}

// Per-CPU wheel selection ported from DragonFlyBSD sys/kern/kern_timeout.c
// `softclock_pcpu_ary` and `callout_reset_bycpu` @ 94448f622a06cde90ba42d688e9cab2448ad9ab3
// (3-clause BSD; The DragonFly Project). Rust reimplementation; no C expression copied.

/// Number of high bits used for the CPU in a cyclic handle.
const CYCLIC_CPU_SHIFT: u32 = 56;
/// Mask of the wheel-local cyclic id in a handle.
const CYCLIC_ID_MASK: u64 = (1u64 << CYCLIC_CPU_SHIFT) - 1;

/// The kernel backend for `raven-cyclic`: records the next deadline and reads [`now_ns`].
///
/// The hardware timer is still periodic (1000 Hz) until the tickless work (L5) reprograms it, so
/// `program` records the requested deadline for diagnostics and the self-check rather than
/// reprogramming the LAPIC/generic timer. The periodic tick stays the fallback interrupt source.
struct CyclicBackend {
    cpu: usize,
}

/// The next deadline for each CPU's wheel (0 = disarmed).
static CYCLIC_NEXT_DEADLINE: [AtomicU64; crate::sched::smp::MAX_CPUS] =
    [const { AtomicU64::new(0) }; crate::sched::smp::MAX_CPUS];

impl cyclic::Backend for CyclicBackend {
    fn program(&mut self, deadline_ns: Option<u64>) {
        if let Some(slot) = CYCLIC_NEXT_DEADLINE.get(self.cpu) {
            slot.store(deadline_ns.unwrap_or(0), Ordering::Release);
        }
    }

    fn now_ns(&self) -> u64 {
        now_ns()
    }
}

/// One wheel per CPU, matching DragonFly `softclock_pcpu_ary`, seeded lazily on first add.
static CYCLICS: [IrqSpinLock<Option<cyclic::Cyclics<CyclicBackend>>>; crate::sched::smp::MAX_CPUS] =
    [const { IrqSpinLock::new(None) }; crate::sched::smp::MAX_CPUS];

/// Fire-order recording for the cyclic self-test (bounded; extra firings are dropped).
static CYCLIC_FIRE_LOG: [AtomicU64; 8] = [const { AtomicU64::new(u64::MAX) }; 8];
static CYCLIC_FIRE_COUNT: AtomicUsize = AtomicUsize::new(0);
/// Firings in the current per-CPU wheel batch (self-test only).
static CYCLIC_FIRE_BATCH: AtomicU64 = AtomicU64::new(0);
/// SMP attribution trace, enabled only by `selftest=cyclic-smp` (fixed capacity, no allocation).
const CYCLIC_TRACE_MAX: usize = 64;
static CYCLIC_TRACE_ENABLED: AtomicBool = AtomicBool::new(false);
static CYCLIC_TRACE_COUNT: AtomicUsize = AtomicUsize::new(0);
static CYCLIC_TRACE_ID: [AtomicU64; CYCLIC_TRACE_MAX] =
    [const { AtomicU64::new(0) }; CYCLIC_TRACE_MAX];
static CYCLIC_TRACE_CPU: [AtomicU64; CYCLIC_TRACE_MAX] =
    [const { AtomicU64::new(0) }; CYCLIC_TRACE_MAX];
static CYCLIC_TRACE_NS: [AtomicU64; CYCLIC_TRACE_MAX] =
    [const { AtomicU64::new(0) }; CYCLIC_TRACE_MAX];

fn wheel_cpu(cpu: usize) -> usize {
    cpu.min(crate::sched::smp::MAX_CPUS.saturating_sub(1))
}

fn encode_cyclic(cpu: usize, id: cyclic::CyclicId) -> cyclic::CyclicId {
    ((cpu as u64) << CYCLIC_CPU_SHIFT) | (id & CYCLIC_ID_MASK)
}

fn decode_cyclic(id: cyclic::CyclicId) -> (usize, cyclic::CyclicId) {
    ((id >> CYCLIC_CPU_SHIFT) as usize, id & CYCLIC_ID_MASK)
}

/// Schedule a cyclic on `cpu`'s wheel (DragonFly `callout_reset_bycpu`).
pub fn cyclic_add_on(cpu: usize, spec: cyclic::CyclicSpec) -> cyclic::CyclicId {
    let cpu = wheel_cpu(cpu);
    let Some(wheel) = CYCLICS.get(cpu) else {
        return 0;
    };
    let mut guard = wheel.lock();
    let registry = guard.get_or_insert_with(|| cyclic::Cyclics::new(CyclicBackend { cpu }));
    encode_cyclic(cpu, registry.add(spec))
}

/// Schedule a cyclic timer on this CPU and return its handle.
pub fn cyclic_add(spec: cyclic::CyclicSpec) -> cyclic::CyclicId {
    cyclic_add_on(crate::sched::smp::percpu::this_cpu(), spec)
}

/// Cancel a cyclic timer (idempotent; `false` when unknown or already fired).
#[allow(dead_code)] // L4 public API; first consumers arrive with R-TIM-9.
pub fn cyclic_cancel(id: cyclic::CyclicId) -> bool {
    let (cpu, local) = decode_cyclic(id);
    let Some(wheel) = CYCLICS.get(cpu) else {
        return false;
    };
    let mut guard = wheel.lock();
    guard
        .as_mut()
        .is_some_and(|registry| registry.cancel(local))
}

/// Fire due cyclics of `cpu`'s wheel and reprogram its next deadline.
///
/// Runs from the timer interrupt (interrupt context); it performs no allocation. L4 has no cyclic
/// consumers yet (R-TIM-9), so the fire callback only records the fire order for the self-test.
fn cyclic_fire_on(cpu: usize) {
    let cpu = wheel_cpu(cpu);
    let Some(wheel) = CYCLICS.get(cpu) else {
        return;
    };
    let mut guard = wheel.lock();
    if let Some(registry) = guard.as_mut() {
        registry.on_interrupt(now_ns(), |id| {
            let global = encode_cyclic(cpu, id);
            CYCLIC_FIRE_BATCH.fetch_add(1, Ordering::Relaxed);
            if CYCLIC_TRACE_ENABLED.load(Ordering::Relaxed) {
                let index = CYCLIC_TRACE_COUNT.fetch_add(1, Ordering::AcqRel);
                if index < CYCLIC_TRACE_MAX {
                    if let Some(slot) = CYCLIC_TRACE_ID.get(index) {
                        slot.store(global, Ordering::Release);
                    }
                    if let Some(slot) = CYCLIC_TRACE_CPU.get(index) {
                        slot.store(cpu as u64, Ordering::Release);
                    }
                    if let Some(slot) = CYCLIC_TRACE_NS.get(index) {
                        slot.store(now_ns(), Ordering::Release);
                    }
                }
            }
            let index = CYCLIC_FIRE_COUNT.fetch_add(1, Ordering::Relaxed);
            if let Some(slot) = CYCLIC_FIRE_LOG.get(index) {
                slot.store(global, Ordering::Relaxed);
            }
        });
    }
}

/// Fire the due cyclics of this CPU's wheel.
fn cyclic_fire() {
    cyclic_fire_on(crate::sched::smp::percpu::this_cpu());
}

/// The recorded next deadline on this CPU (0 when disarmed); for diagnostics.
#[allow(dead_code)] // Diagnostic accessor for the tickless (L5) work.
pub fn cyclic_next_deadline() -> u64 {
    CYCLIC_NEXT_DEADLINE
        .get(wheel_cpu(crate::sched::smp::percpu::this_cpu()))
        .map_or(0, |slot| slot.load(Ordering::Acquire))
}

/// Boot-time self-check: three one-shots fire in deadline order with no lost wake.
///
/// Runs from [`init`] before interrupts are enabled, advancing the monotonic clock with
/// [`delay_ns`] and firing through [`cyclic_fire`], so the deadline ordering is checked
/// independently of the periodic tick. Deadlines are spaced 5 ms apart so the TSC-scale precision
/// and QEMU timing jitter cannot move a fire across a boundary.
fn cyclic_selftest() {
    let base = now_ns();
    if base == 0 {
        kprintln!("cyclic: skip (no monotonic clock)");
        return;
    }
    CYCLIC_FIRE_COUNT.store(0, Ordering::Relaxed);
    let a = cyclic_add(cyclic::CyclicSpec {
        when: base.saturating_add(15_000_000),
        interval: 0,
        level: cyclic::Level::Normal,
        slack_ns: 0,
    });
    let b = cyclic_add(cyclic::CyclicSpec {
        when: base.saturating_add(5_000_000),
        interval: 0,
        level: cyclic::Level::Normal,
        slack_ns: 0,
    });
    let c = cyclic_add(cyclic::CyclicSpec {
        when: base.saturating_add(10_000_000),
        interval: 0,
        level: cyclic::Level::Normal,
        slack_ns: 0,
    });
    let end = base.saturating_add(25_000_000);
    let mut steps = 0u32;
    while CYCLIC_FIRE_COUNT.load(Ordering::Relaxed) < 3 && now_ns() < end && steps < 500 {
        delay_ns(500_000);
        cyclic_fire();
        steps = steps.saturating_add(1);
    }
    cyclic_fire(); // catch anything that became due at the loop boundary
    let count = CYCLIC_FIRE_COUNT.load(Ordering::Relaxed);
    let first = CYCLIC_FIRE_LOG
        .first()
        .map_or(u64::MAX, |slot| slot.load(Ordering::Relaxed));
    let second = CYCLIC_FIRE_LOG
        .get(1)
        .map_or(u64::MAX, |slot| slot.load(Ordering::Relaxed));
    let third = CYCLIC_FIRE_LOG
        .get(2)
        .map_or(u64::MAX, |slot| slot.load(Ordering::Relaxed));
    if count == 3 && first == b && second == c && third == a {
        kprintln!("cyclic: ok fired=3 order=5ms,10ms,15ms");
    } else {
        kprintln!("cyclic: fail fired={count} order={first},{second},{third}");
        boot::exit_qemu(0x11);
    }

    // Per-CPU wheel isolation: a due timer armed for CPU 1 must not fire from CPU 0's wheel.
    // Both deadlines are in the past when the first wheel fires, so a single shared wheel would
    // drain both here and leave nothing for CPU 1.
    let cpu_base = now_ns();
    let _cpu0 = cyclic_add_on(
        0,
        cyclic::CyclicSpec {
            when: cpu_base.saturating_add(5_000_000),
            interval: 0,
            level: cyclic::Level::Normal,
            slack_ns: 0,
        },
    );
    let _cpu1 = cyclic_add_on(
        1,
        cyclic::CyclicSpec {
            when: cpu_base.saturating_add(10_000_000),
            interval: 0,
            level: cyclic::Level::Normal,
            slack_ns: 0,
        },
    );
    delay_ns(20_000_000);
    CYCLIC_FIRE_BATCH.store(0, Ordering::Relaxed);
    cyclic_fire_on(0);
    let cpu0 = CYCLIC_FIRE_BATCH.swap(0, Ordering::Relaxed);
    cyclic_fire_on(1);
    let cpu1 = CYCLIC_FIRE_BATCH.swap(0, Ordering::Relaxed);
    if cpu0 == 1 && cpu1 == 1 {
        kprintln!("cyclic: percpu ok cpu0={cpu0} cpu1={cpu1}");
    } else {
        kprintln!("cyclic: percpu fail cpu0={cpu0} cpu1={cpu1}");
        boot::exit_qemu(0x12);
    }
}

/// Timer capacity of the SMP attribution test.
const CYCLIC_SMP_MAX: usize = 16;
/// A fire later than this after its deadline fails the SMP attribution row.
const CYCLIC_SMP_WINDOW_NS: u64 = 100_000_000;
/// Overall wait for the SMP attribution row (all deadlines are <= 160 ms).
const CYCLIC_SMP_WAIT_NS: u64 = 500_000_000;
static CYCLIC_SMP_STOP: AtomicBool = AtomicBool::new(false);
static CYCLIC_SMP_HOPS: AtomicU64 = AtomicU64::new(0);

fn cyclic_smp_spec(when: u64) -> cyclic::CyclicSpec {
    cyclic::CyclicSpec {
        when,
        interval: 0,
        level: cyclic::Level::Normal,
        slack_ns: 0,
    }
}

fn cyclic_smp_load() {
    while !CYCLIC_SMP_STOP.load(Ordering::Acquire) {
        CYCLIC_SMP_HOPS.fetch_add(1, Ordering::Relaxed);
        core::hint::spin_loop();
    }
}

fn cyclic_smp_record(
    active: &mut [(u64, usize, u64); CYCLIC_SMP_MAX],
    count: &mut usize,
    id: u64,
    cpu: usize,
    deadline: u64,
) {
    if let Some(slot) = active.get_mut(*count) {
        *slot = (id, cpu, deadline);
        *count = (*count).saturating_add(1);
    }
}

/// `selftest=cyclic-smp`: prove each per-CPU wheel fires on its own CPU under ``-smp 4`` load.
///
/// Arms a short, a long, a cancelled and a re-armed timer on every online CPU. One busy kernel
/// load worker per CPU runs during the window. A fire must match its owner CPU, occur once, and
/// stay inside [`CYCLIC_SMP_WINDOW_NS`]; a cancelled id must never fire. The BSP wait uses the
/// scheduler timeout path, so a lost wake would also surface here.
pub fn cyclic_smp_selftest() {
    let cpus = crate::sched::smp::online_cpus().min(4);
    let base = now_ns();
    if base == 0 {
        kprintln!("cyclic-smp: skip (no clock)");
        return;
    }
    if cpus < 2 {
        kprintln!("cyclic-smp: skip cpus={cpus}");
        return;
    }

    CYCLIC_SMP_STOP.store(false, Ordering::Release);
    CYCLIC_SMP_HOPS.store(0, Ordering::Relaxed);
    let mut load_workers = 0usize;
    for cpu in 0..cpus {
        if crate::sched::spawn_kernel_on(cpu, "cyclic-smp-load", cyclic_smp_load).is_ok() {
            load_workers = load_workers.saturating_add(1);
        }
    }
    if load_workers != cpus {
        kprintln!("cyclic-smp: fail load_workers={load_workers} cpus={cpus}");
        boot::exit_qemu(0x14);
    }

    CYCLIC_TRACE_COUNT.store(0, Ordering::Release);
    CYCLIC_TRACE_ENABLED.store(true, Ordering::Release);
    let mut active = [(0u64, 0usize, 0u64); CYCLIC_SMP_MAX];
    let mut active_count = 0usize;
    let mut cancelled = [0u64; 4];
    for cpu in 0..cpus {
        let short_deadline = base.saturating_add(30_000_000);
        let short = cyclic_add_on(cpu, cyclic_smp_spec(short_deadline));
        cyclic_smp_record(&mut active, &mut active_count, short, cpu, short_deadline);

        let long_deadline = base.saturating_add(120_000_000);
        let long = cyclic_add_on(cpu, cyclic_smp_spec(long_deadline));
        cyclic_smp_record(&mut active, &mut active_count, long, cpu, long_deadline);

        let cancelled_id = cyclic_add_on(cpu, cyclic_smp_spec(base.saturating_add(60_000_000)));
        if let Some(slot) = cancelled.get_mut(cpu) {
            *slot = cancelled_id;
        }
        if !cyclic_cancel(cancelled_id) {
            kprintln!("cyclic-smp: fail cancel armed cpu={cpu}");
            boot::exit_qemu(0x13);
        }

        let old = cyclic_add_on(cpu, cyclic_smp_spec(base.saturating_add(50_000_000)));
        if !cyclic_cancel(old) {
            kprintln!("cyclic-smp: fail cancel rearm cpu={cpu}");
            boot::exit_qemu(0x13);
        }
        let rearm_deadline = base.saturating_add(160_000_000);
        let rearm = cyclic_add_on(cpu, cyclic_smp_spec(rearm_deadline));
        cyclic_smp_record(&mut active, &mut active_count, rearm, cpu, rearm_deadline);
    }

    let end = base.saturating_add(CYCLIC_SMP_WAIT_NS);
    while CYCLIC_TRACE_COUNT.load(Ordering::Acquire) < active_count && now_ns() < end {
        crate::sched::sleep::sleep_ns(1_000_000);
    }
    CYCLIC_TRACE_ENABLED.store(false, Ordering::Release);

    let trace_count = CYCLIC_TRACE_COUNT
        .load(Ordering::Acquire)
        .min(CYCLIC_TRACE_MAX);
    let mut delays = [0u64; CYCLIC_SMP_MAX];
    let mut delay_count = 0usize;
    let mut per_cpu = [0usize; 4];
    let mut fired = 0usize;
    let mut wrong = 0usize;
    let mut lost = 0usize;
    let mut dup = 0usize;
    let mut early = 0usize;
    let mut late = 0usize;
    for &(id, owner, deadline) in active.get(..active_count).unwrap_or(&[]) {
        let mut hits = 0usize;
        let mut fire_cpu = 0usize;
        let mut fire_ns = 0u64;
        for index in 0..trace_count {
            let trace_id = CYCLIC_TRACE_ID
                .get(index)
                .map_or(u64::MAX, |slot| slot.load(Ordering::Acquire));
            if trace_id != id {
                continue;
            }
            hits = hits.saturating_add(1);
            fire_cpu = CYCLIC_TRACE_CPU
                .get(index)
                .map_or(0, |slot| slot.load(Ordering::Acquire)) as usize;
            fire_ns = CYCLIC_TRACE_NS
                .get(index)
                .map_or(0, |slot| slot.load(Ordering::Acquire));
        }
        if hits == 0 {
            lost = lost.saturating_add(1);
            continue;
        }
        if hits > 1 {
            dup = dup.saturating_add(1);
            continue;
        }
        fired = fired.saturating_add(1);
        if fire_cpu != owner {
            wrong = wrong.saturating_add(1);
        }
        if fire_ns < deadline {
            early = early.saturating_add(1);
        }
        let delay = fire_ns.saturating_sub(deadline);
        if delay > CYCLIC_SMP_WINDOW_NS {
            late = late.saturating_add(1);
        }
        if let Some(slot) = delays.get_mut(delay_count) {
            *slot = delay;
            delay_count = delay_count.saturating_add(1);
        }
        if let Some(slot) = per_cpu.get_mut(owner) {
            *slot = (*slot).saturating_add(1);
        }
    }

    let mut cancelled_fired = 0usize;
    for &id in cancelled.get(..cpus).unwrap_or(&[]) {
        for index in 0..trace_count {
            if CYCLIC_TRACE_ID
                .get(index)
                .is_some_and(|slot| slot.load(Ordering::Acquire) == id)
            {
                cancelled_fired = cancelled_fired.saturating_add(1);
            }
        }
    }

    for index in 1..delay_count {
        let mut cursor = index;
        while cursor > 0
            && delays
                .get(cursor)
                .zip(delays.get(cursor.saturating_sub(1)))
                .is_some_and(|(current, previous)| current < previous)
        {
            delays.swap(cursor, cursor.saturating_sub(1));
            cursor = cursor.saturating_sub(1);
        }
    }

    let min_us = delays.first().copied().unwrap_or(0) / 1_000;
    let p50_us = delays.get(delay_count / 2).copied().unwrap_or(0) / 1_000;
    let max_us = delays
        .get(delay_count.saturating_sub(1))
        .copied()
        .unwrap_or(0)
        / 1_000;
    let hops = CYCLIC_SMP_HOPS.load(Ordering::Relaxed);
    let cpu0 = per_cpu.first().copied().unwrap_or(0);
    let cpu1 = per_cpu.get(1).copied().unwrap_or(0);
    let cpu2 = per_cpu.get(2).copied().unwrap_or(0);
    let cpu3 = per_cpu.get(3).copied().unwrap_or(0);
    let ok = fired == active_count
        && wrong == 0
        && lost == 0
        && dup == 0
        && early == 0
        && late == 0
        && cancelled_fired == 0;

    if ok {
        kprintln!(
            "cyclic-smp: ok cpus={cpus} expected={active_count} fired={fired} cancelled={cpus} wrong=0 lost=0 dup=0 early=0 late=0 min_us={min_us} p50_us={p50_us} max_us={max_us} load_workers={load_workers} load_hops={hops} cpu0={cpu0} cpu1={cpu1} cpu2={cpu2} cpu3={cpu3}"
        );
    } else {
        kprintln!(
            "cyclic-smp: fail cpus={cpus} expected={active_count} fired={fired} cancelled_fired={cancelled_fired} wrong={wrong} lost={lost} dup={dup} early={early} late={late} min_us={min_us} p50_us={p50_us} max_us={max_us} load_workers={load_workers} load_hops={hops}"
        );
        boot::exit_qemu(0x13);
    }

    CYCLIC_SMP_STOP.store(true, Ordering::Release);
    crate::sched::sleep::sleep_ns(10_000_000);
}

/// Uptime as `(seconds, nanoseconds)`.
#[allow(dead_code)] // Consumed by the S09 scheduler and later servers.
pub fn uptime() -> (u64, u32) {
    let now = now_ns();
    (now / 1_000_000_000, (now % 1_000_000_000) as u32)
}

/// Realtime clock at boot: the RTC epoch, read lazily on first use.
///
/// Returns zero when the platform has no usable RTC, which keeps `CLOCK_REALTIME` at zero rather
/// than blocking boot. The RTC transcript is printed once, on the first call.
pub fn boot_realtime_ns() -> u64 {
    if !REALTIME_READ.load(Ordering::Acquire) {
        read_rtc_once();
    }
    BOOT_REALTIME_NS.load(Ordering::Acquire)
}

/// Wall-clock nanoseconds since the Unix epoch (`boot_realtime_ns()` plus monotonic elapsed).
pub fn realtime_ns() -> u64 {
    boot_realtime_ns().saturating_add(now_ns())
}

/// Set the wall clock to `ns` (the `CLOCK_SET` syscall).
///
/// Re-anchors the boot offset so that [`realtime_ns`] returns `ns` at this instant and keeps
/// advancing with the monotonic clock.
pub fn set_realtime_ns(ns: u64) {
    BOOT_REALTIME_NS.store(ns.saturating_sub(now_ns()), Ordering::Release);
    REALTIME_READ.store(true, Ordering::Release);
}

/// Read the platform RTC once and print the transcript.
fn read_rtc_once() {
    match rtc::read() {
        Some(epoch_ns) => {
            BOOT_REALTIME_NS.store(epoch_ns, Ordering::Release);
            log_rtc(epoch_ns);
        }
        None => {
            kprintln!("rtc: unavailable");
        }
    }
    REALTIME_READ.store(true, Ordering::Release);
}

/// Print the RTC transcript (ISO-like date and epoch seconds).
fn log_rtc(epoch_ns: u64) {
    let seconds = epoch_ns / 1_000_000_000;
    let days = (seconds / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let tod = seconds % 86_400;
    let hour = tod / 3600;
    let minute = (tod % 3600) / 60;
    let second = tod % 60;
    kprintln!(
        "rtc: {year}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} (epoch {seconds}s)"
    );
}

/// Days since 1970-01-01 from a civil date (Howard Hinnant's `days_from_civil`).
#[cfg(target_arch = "x86_64")]
#[allow(clippy::arithmetic_side_effects)] // Pure bounded civil-date math on i64; cannot overflow for valid dates.
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date `(year, month, day)` from days since 1970-01-01 (Hinnant's `civil_from_days`).
#[allow(clippy::arithmetic_side_effects)] // Pure bounded civil-date math on i64; cannot overflow for valid dates.
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Busy-wait for `ns` nanoseconds using the monotonic clock.
#[allow(dead_code)] // Used by later kernel init and the S09 scheduler.
pub fn delay_ns(ns: u64) {
    if ns == 0 {
        return;
    }
    let start = now_ns();
    if start == 0 {
        return;
    }
    let target = start.saturating_add(ns);
    while now_ns() < target {
        core::hint::spin_loop();
    }
}

/// Wait for the next interrupt.
#[inline]
fn wait_for_interrupt() {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: interrupts are enabled before the self-test; `hlt` wakes on the timer tick.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: interrupts are enabled before the self-test; `wfi` wakes on the timer IRQ.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Run the `selftest=timer` checks.
pub fn selftest() {
    let start_ticks = ticks();
    let start_ns = now_ns();
    let target = start_ticks.saturating_add(100);
    while ticks() < target {
        wait_for_interrupt();
    }
    let elapsed = now_ns().saturating_sub(start_ns);
    let elapsed_ms = elapsed / 1_000_000;
    kprintln!("selftest: timer 100 ticks in {elapsed_ms} ms");
    #[cfg(target_arch = "aarch64")]
    kprintln!("selftest: timer interrupts={}", crate::arch::irq::count());

    let mut previous = now_ns();
    let mut index = 0u32;
    while index < 1000 {
        let current = now_ns();
        if current < previous {
            kprintln!("selftest: timer monotonicity failed");
            boot::exit_qemu(0x11);
        }
        previous = current;
        index = index.saturating_add(1);
    }

    if !(70..=130).contains(&elapsed_ms) {
        kprintln!("selftest: timer interval out of range");
        boot::exit_qemu(0x11);
    }
}

/// The detected counter frequency, for diagnostics.
#[allow(dead_code)] // Diagnostic helper for later tickets.
pub fn tsc_hz() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        TSC_HZ.load(Ordering::Acquire)
    }
    #[cfg(target_arch = "aarch64")]
    {
        COUNTER_HZ.load(Ordering::Acquire)
    }
}

// ---------------------------------------------------------------------------
// Entropy
// ---------------------------------------------------------------------------

/// The global pool, seeded once at boot from a hardware source.
static ENTROPY_POOL: IrqSpinLock<Option<raven_entropy::Pool>> = IrqSpinLock::new(None);
/// Whether the boot seed pass has run.
static ENTROPY_INIT: AtomicBool = AtomicBool::new(false);

/// The entropy source the pool was seeded from.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Variants are arch-gated, so one is always unused on a given architecture.
enum EntropySource {
    Rdrand,
    Rndr,
    Jitter,
}

impl EntropySource {
    fn name(self) -> &'static str {
        match self {
            EntropySource::Rdrand => "rdrand",
            EntropySource::Rndr => "rndr",
            EntropySource::Jitter => "jitter",
        }
    }

    const fn is_weak(self) -> bool {
        matches!(self, EntropySource::Jitter)
    }
}

/// Measured timer-jitter bound for the software seed.
#[derive(Clone, Copy)]
struct JitterStats {
    samples: u32,
    nonzero: u32,
    distinct: u32,
    min_delta: u64,
    max_delta: u64,
    rtc: bool,
}

/// Fill `out` with pool bytes.
///
/// The pool is seeded once at boot from RDRAND/RNDR; a platform without either uses the measured
/// timer/RTC jitter fallback through [`raven_entropy::Pool::stir_weak`]. A pool that exhausts its
/// re-key budget is stirred from the same ordered source selection. A truly source-less platform
/// returns [`raven_entropy::Error::Unseeded`]/[`raven_entropy::Error::NeedsReseed`] and `GETRANDOM`
/// maps that to `EAGAIN`. The jitter source is explicitly weak and is never reported as
/// cryptographic.
pub fn entropy_fill(out: &mut [u8]) -> Result<usize, raven_entropy::Error> {
    let mut guard = ENTROPY_POOL.lock();
    let Some(pool) = guard.as_mut() else {
        return Err(raven_entropy::Error::Unseeded);
    };
    match pool.fill(out) {
        Ok(written) => Ok(written),
        Err(raven_entropy::Error::NeedsReseed) => {
            let Some((seed, source, _stats)) = seed_source() else {
                return Err(raven_entropy::Error::NeedsReseed);
            };
            if source.is_weak() {
                pool.stir_weak(&seed)?;
            } else {
                pool.stir(raven_entropy::SeedSource::Hardware, &seed)?;
            }
            pool.fill(out)
        }
        Err(error) => Err(error),
    }
}

/// Seed the pool once at boot; the boot log records the source or `none`.
fn entropy_init() {
    if ENTROPY_INIT.swap(true, Ordering::AcqRel) {
        return;
    }
    if !entropy_disabled() {
        let mut pool = raven_entropy::Pool::new();
        if let Some((seed, source, stats)) = seed_source() {
            let result = if source.is_weak() {
                pool.stir_weak(&seed)
            } else {
                pool.stir(raven_entropy::SeedSource::Hardware, &seed)
            };
            if result.is_ok() {
                *ENTROPY_POOL.lock() = Some(pool);
                kprintln!("entropy: source={}", source.name());
                if let Some(stats) = stats {
                    kprintln!(
                        "entropy: jitter samples={} nonzero={} distinct={} min={} max={} rtc={}",
                        stats.samples,
                        stats.nonzero,
                        stats.distinct,
                        stats.min_delta,
                        stats.max_delta,
                        u8::from(stats.rtc)
                    );
                }
                return;
            }
        }
    }
    kprintln!("entropy: source=none");
}

/// `entropy=none` disables both hardware and software seeding for the unseeded contract test.
fn entropy_disabled() -> bool {
    crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "entropy=none")
}

/// Ordered seed selection: hardware first, then the measured software jitter fallback.
fn seed_source() -> Option<(
    [u8; raven_entropy::SEED_SIZE],
    EntropySource,
    Option<JitterStats>,
)> {
    if let Some((seed, source)) = hardware_seed() {
        return Some((seed, source, None));
    }
    let (seed, stats) = software_seed()?;
    Some((seed, EntropySource::Jitter, Some(stats)))
}

/// Sample a hardware entropy source and return the first 40 bytes as a seed.
fn hardware_seed() -> Option<([u8; raven_entropy::SEED_SIZE], EntropySource)> {
    #[cfg(target_arch = "x86_64")]
    if let Some(bytes) = rdrand_entropy() {
        return Some((seed_from(&bytes), EntropySource::Rdrand));
    }
    #[cfg(target_arch = "aarch64")]
    if let Some(bytes) = rndr_entropy() {
        return Some((seed_from(&bytes), EntropySource::Rndr));
    }
    None
}

fn seed_from(bytes: &[u8; 64]) -> [u8; raven_entropy::SEED_SIZE] {
    let mut seed = [0u8; raven_entropy::SEED_SIZE];
    seed.copy_from_slice(&bytes[..raven_entropy::SEED_SIZE]);
    seed
}

/// Collect the measured timer/RTC jitter fallback.
///
/// The sample count and observed delta range are returned so the boot log and report state the
/// actual bound. There is no claim that this is a cryptographic source; it only keeps ASLR and
/// early boot from using one fixed layout when the board has no RDRAND/RNDR.
fn software_seed() -> Option<([u8; raven_entropy::SEED_SIZE], JitterStats)> {
    const SAMPLES: usize = 64;
    let mut seed = [0u8; raven_entropy::SEED_SIZE];
    let mut deltas = [0u64; SAMPLES];
    let mut previous = timer_counter();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut index = 0usize;
    let mut min_delta = u64::MAX;
    let mut max_delta = 0u64;
    let mut nonzero = 0u32;
    while index < SAMPLES {
        for _ in 0..8 {
            core::hint::spin_loop();
        }
        let current = timer_counter();
        let delta = current.wrapping_sub(previous);
        previous = current;
        // `index < SAMPLES` is the loop bound; the checked access keeps the invariant explicit.
        if let Some(sample) = deltas.get_mut(index) {
            *sample = delta;
        }
        min_delta = min_delta.min(delta);
        max_delta = max_delta.max(delta);
        if delta != 0 {
            nonzero = nonzero.saturating_add(1);
        }
        state ^= delta.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        state = state.rotate_left(17) ^ (state >> 7);
        let word = state ^ delta.rotate_left(23) ^ (current >> 17);
        let offset = (index % 5).saturating_mul(8);
        for (byte_index, byte) in word.to_le_bytes().iter().enumerate() {
            let slot = offset.wrapping_add(byte_index) % raven_entropy::SEED_SIZE;
            if let Some(entry) = seed.get_mut(slot) {
                *entry ^= byte;
            }
        }
        index = index.saturating_add(1);
    }

    let mut distinct = 0u32;
    for current in 0..SAMPLES {
        if let (Some(prefix), Some(value)) = (deltas.get(..current), deltas.get(current))
            && !prefix.contains(value)
        {
            distinct = distinct.saturating_add(1);
        }
    }

    let rtc = rtc::read();
    if let Some(ns) = rtc {
        let bytes = ns.to_le_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            let slot = index % raven_entropy::SEED_SIZE;
            if let Some(entry) = seed.get_mut(slot) {
                *entry ^= byte;
            }
        }
        let word = state.rotate_left(29) ^ ns.rotate_left(11);
        for (index, byte) in word.to_le_bytes().iter().enumerate() {
            let slot = index.wrapping_add(32) % raven_entropy::SEED_SIZE;
            if let Some(entry) = seed.get_mut(slot) {
                *entry ^= byte;
            }
        }
    }

    if nonzero == 0 && rtc.is_none() {
        return None;
    }
    if seed.iter().all(|byte| *byte == 0) {
        seed[0] = 0xA5;
    }
    let stats = JitterStats {
        samples: u32::try_from(SAMPLES).unwrap_or(u32::MAX),
        nonzero,
        distinct,
        min_delta,
        max_delta,
        rtc: rtc.is_some(),
    };
    Some((seed, stats))
}

/// The platform counter used for the jitter samples.
#[cfg(target_arch = "x86_64")]
fn timer_counter() -> u64 {
    tsc::read()
}

/// The platform counter used for the jitter samples.
#[cfg(target_arch = "aarch64")]
fn timer_counter() -> u64 {
    arch_timer::read_counter()
}

/// Read 64 bytes from RDRAND when the CPUID feature bit is present.
#[cfg(target_arch = "x86_64")]
fn rdrand_entropy() -> Option<[u8; 64]> {
    // RDRAND is present when CPUID.1:ECX[30] is set.
    let features = core::arch::x86_64::__cpuid(1);
    if features.ecx & (1 << 30) == 0 {
        return None;
    }
    let mut bytes = [0u8; 64];
    for chunk in bytes.chunks_exact_mut(8) {
        let value = rdrand64()?;
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    Some(bytes)
}

/// One RDRAND word with a short retry loop (RDRAND may set CF=0 under contention).
#[cfg(target_arch = "x86_64")]
fn rdrand64() -> Option<u64> {
    for _ in 0..10 {
        let mut value: u64;
        let mut ok: u8;
        // SAFETY: `rdrand` is available per the CPUID.1:ECX[30] check and has no memory side
        // effects; `setc` records the success flag. `value` is only used when `ok` is set.
        unsafe {
            core::arch::asm!(
                "rdrand {value}",
                "setc {ok}",
                value = out(reg) value,
                ok = out(reg_byte) ok,
                options(nomem, nostack),
            );
        }
        if ok != 0 {
            return Some(value);
        }
    }
    None
}

/// Read 64 bytes from RNDR when the architecture feature is present.
#[cfg(target_arch = "aarch64")]
fn rndr_entropy() -> Option<[u8; 64]> {
    if !rndr_available() {
        return None;
    }
    let mut bytes = [0u8; 64];
    for chunk in bytes.chunks_exact_mut(8) {
        let mut value: u64;
        // SAFETY: RNDR is a read-only system register enabled by the `ID_AA64ISAR0_EL1` check in
        // `rndr_available`; it has no memory side effects.
        unsafe {
            core::arch::asm!(
                "mrs {}, s3_3_c2_c4_0",
                out(reg) value,
                options(nomem, nostack, preserves_flags),
            );
        }
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    Some(bytes)
}

/// Whether the CPU implements RNDR (`ID_AA64ISAR0_EL1[63:60] == 1`).
#[cfg(target_arch = "aarch64")]
fn rndr_available() -> bool {
    let isar: u64;
    // SAFETY: ID_AA64ISAR0_EL1 is a read-only system register readable at EL1.
    unsafe {
        core::arch::asm!(
            "mrs {}, id_aa64isar0_el1",
            out(reg) isar,
            options(nomem, nostack, preserves_flags),
        );
    }
    (isar >> 60) & 0xf == 1
}
