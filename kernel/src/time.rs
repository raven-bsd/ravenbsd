// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Monotonic kernel time from the TSC and the timer tick counter.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raven_timemath::{self, Scale};

use crate::arch::x86_64::{apic, tsc};
use crate::{boot, kprintln};

static TSC_HZ: AtomicU64 = AtomicU64::new(0);
static BOOT_TSC: AtomicU64 = AtomicU64::new(0);
static TSC_MULT: AtomicU64 = AtomicU64::new(0);
static TSC_SHIFT: AtomicU32 = AtomicU32::new(0);
static TICKS: AtomicU64 = AtomicU64::new(0);

/// Initialise the monotonic clock.
pub fn init(tsc_hz: u64, boot_tsc: u64) {
    TSC_HZ.store(tsc_hz, Ordering::Release);
    BOOT_TSC.store(boot_tsc, Ordering::Release);
    if let Some(scale) = raven_timemath::scale_for_tsc(tsc_hz) {
        TSC_MULT.store(scale.mult, Ordering::Release);
        TSC_SHIFT.store(scale.shift, Ordering::Release);
    }
}

/// Nanoseconds since [`init`].
pub fn now_ns() -> u64 {
    let mult = TSC_MULT.load(Ordering::Acquire);
    let shift = TSC_SHIFT.load(Ordering::Acquire);
    if mult == 0 {
        return 0;
    }
    let ticks = tsc::read().saturating_sub(BOOT_TSC.load(Ordering::Acquire));
    raven_timemath::tsc_to_ns(ticks, Scale { mult, shift })
}

/// Timer ticks since boot.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Acquire)
}

/// Called from the timer interrupt.
pub fn on_tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    apic::eoi();
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

/// Busy-wait for `ns` nanoseconds using the TSC.
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

/// Run the `selftest=timer` checks.
pub fn selftest() {
    let start_ticks = ticks();
    let start_ns = now_ns();
    let target = start_ticks.saturating_add(100);
    while ticks() < target {
        // SAFETY: interrupts are enabled before the self-test; `hlt` wakes on the timer tick.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
    let elapsed = now_ns().saturating_sub(start_ns);
    let elapsed_ms = elapsed / 1_000_000;
    kprintln!("selftest: timer 100 ticks in {elapsed_ms} ms");

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

/// The detected TSC frequency, for diagnostics.
#[allow(dead_code)] // Diagnostic helper for later tickets.
pub fn tsc_hz() -> u64 {
    TSC_HZ.load(Ordering::Acquire)
}
