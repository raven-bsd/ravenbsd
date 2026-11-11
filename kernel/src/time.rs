// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Monotonic kernel time from the architecture counter and the timer tick counter.
//!
//! x86-64 uses the TSC with a calibrated scale; aarch64 uses `CNTPCT_EL0` with the frequency from
//! `CNTFRQ_EL0` (or `BootInfo.counter_freq`). Both expose the same interface to the scheduler and
//! the self-tests.

#[cfg(target_arch = "x86_64")]
use core::sync::atomic::AtomicU32;
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_arch = "x86_64")]
use raven_timemath::{self, Scale};

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::timer as arch_timer;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::{apic, tsc};
use crate::{boot, kprintln};

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
    #[cfg(target_arch = "x86_64")]
    apic::eoi();
    // S27 fallback: drain COM1 from the tick when the IOAPIC line is unavailable.
    #[cfg(target_arch = "x86_64")]
    crate::console::poll();
    crate::event::notify_all();
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
