// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Monotonic kernel time from the architecture counter and the timer tick counter.
//!
//! x86-64 uses the TSC with a calibrated scale; aarch64 uses `CNTPCT_EL0` with the frequency from
//! `CNTFRQ_EL0` (or `BootInfo.counter_freq`). Both expose the same interface to the scheduler and
//! the self-tests.

#[cfg(target_arch = "x86_64")]
use core::sync::atomic::AtomicU32;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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
    crate::event::notify_all();
}

// ---------------------------------------------------------------------------
// Cyclic timer integration (L4)
// ---------------------------------------------------------------------------

/// The kernel backend for `raven-cyclic`: records the next deadline and reads [`now_ns`].
///
/// The hardware timer is still periodic (1000 Hz) until the tickless work (L5) reprograms it, so
/// `program` records the requested deadline for diagnostics and the self-check rather than
/// reprogramming the LAPIC/generic timer. The periodic tick stays the fallback interrupt source.
struct CyclicBackend;

/// The next deadline the backend was last programmed for (0 = disarmed).
static CYCLIC_NEXT_DEADLINE: AtomicU64 = AtomicU64::new(0);

impl cyclic::Backend for CyclicBackend {
    fn program(&mut self, deadline_ns: Option<u64>) {
        CYCLIC_NEXT_DEADLINE.store(deadline_ns.unwrap_or(0), Ordering::Release);
    }

    fn now_ns(&self) -> u64 {
        now_ns()
    }
}

/// The global per-CPU cyclic registry, seeded lazily on first [`cyclic_add`].
static CYCLICS: IrqSpinLock<Option<cyclic::Cyclics<CyclicBackend>>> = IrqSpinLock::new(None);

/// Fire-order recording for the cyclic self-test (bounded; extra firings are dropped).
static CYCLIC_FIRE_LOG: [AtomicU64; 8] = [const { AtomicU64::new(u64::MAX) }; 8];
static CYCLIC_FIRE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Schedule a cyclic timer on this CPU and return its handle.
pub fn cyclic_add(spec: cyclic::CyclicSpec) -> cyclic::CyclicId {
    let mut guard = CYCLICS.lock();
    let registry = guard.get_or_insert_with(|| cyclic::Cyclics::new(CyclicBackend));
    registry.add(spec)
}

/// Cancel a cyclic timer (idempotent; `false` when unknown or already fired).
#[allow(dead_code)] // L4 public API; first consumers arrive with R-TIM-9.
pub fn cyclic_cancel(id: cyclic::CyclicId) -> bool {
    let mut guard = CYCLICS.lock();
    guard.as_mut().is_some_and(|registry| registry.cancel(id))
}

/// Fire due cyclics and reprogram the next deadline.
///
/// Runs from the timer interrupt (interrupt context); it performs no allocation. L4 has no cyclic
/// consumers yet (R-TIM-9), so the fire callback only records the fire order for the self-test.
fn cyclic_fire() {
    let mut guard = CYCLICS.lock();
    if let Some(registry) = guard.as_mut() {
        registry.on_interrupt(now_ns(), |id| {
            let index = CYCLIC_FIRE_COUNT.fetch_add(1, Ordering::Relaxed);
            if let Some(slot) = CYCLIC_FIRE_LOG.get(index) {
                slot.store(id, Ordering::Relaxed);
            }
        });
    }
}

/// The recorded next deadline (0 when disarmed); for the self-test and diagnostics.
#[allow(dead_code)] // Diagnostic accessor for the tickless (L5) work.
pub fn cyclic_next_deadline() -> u64 {
    CYCLIC_NEXT_DEADLINE.load(Ordering::Acquire)
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
}

/// Uptime as `(seconds, nanoseconds)`.
#[allow(dead_code)] // Consumed by the S09 scheduler and later servers.
pub fn uptime() -> (u64, u32) {
    let now = now_ns();
    (now / 1_000_000_000, (now % 1_000_000_000) as u32)
}

/// Realtime clock at boot; zero until an RTC source is wired in.
#[allow(dead_code)] // ABI placeholder until the RTC ticket.
pub const fn boot_realtime_ns() -> u64 {
    0
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
