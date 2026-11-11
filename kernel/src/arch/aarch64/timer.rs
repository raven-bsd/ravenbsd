// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ARM generic timer: the non-secure physical timer (`CNTP_*`) at 1000 Hz.
//!
//! The kernel runs at EL1 with no hypervisor, so the EL1 physical timer is directly accessible.
//! `CNTP_TVAL_EL0` is rewritten from the interrupt handler on every tick (one-shot re-arm), which
//! keeps the period exact without a separate comparator bookkeeping path.

use core::arch::asm;
use core::fmt;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Timer interrupt frequency.
pub const HZ: u32 = 1000;

/// Reload value in counter ticks (`freq / HZ`), zero before [`init`].
static RELOAD: AtomicU64 = AtomicU64::new(0);
/// Counter frequency in Hz, zero before [`init`].
static FREQ: AtomicU64 = AtomicU64::new(0);
/// Non-secure physical timer PPI number, zero before [`set_irq`].
static IRQ: AtomicU32 = AtomicU32::new(0);

/// Why timer initialisation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerError {
    /// `CNTFRQ_EL0` is zero.
    ZeroFrequency,
    /// The frequency is too low for a 1 kHz tick.
    FrequencyTooLow,
}

impl fmt::Display for TimerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimerError::ZeroFrequency => f.write_str("generic timer frequency is zero"),
            TimerError::FrequencyTooLow => f.write_str("generic timer frequency is too low"),
        }
    }
}

/// `CNTFRQ_EL0` in Hz.
pub fn frequency() -> u64 {
    let value: u64;
    // SAFETY: CNTFRQ_EL0 is readable at EL1.
    unsafe {
        asm!("mrs {}, cntfrq_el0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// `CNTPCT_EL0`: the monotonic physical counter.
pub fn read_counter() -> u64 {
    let value: u64;
    // SAFETY: CNTPCT_EL0 is readable at EL1.
    unsafe {
        asm!("mrs {}, cntpct_el0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// Program the timer for a 1 kHz period and start it.
pub fn init(irq: u32) -> Result<u64, TimerError> {
    let freq = frequency();
    if freq == 0 {
        return Err(TimerError::ZeroFrequency);
    }
    let reload = freq.checked_div(u64::from(HZ)).unwrap_or(0);
    if reload == 0 {
        return Err(TimerError::FrequencyTooLow);
    }
    FREQ.store(freq, Ordering::Release);
    RELOAD.store(reload, Ordering::Release);
    IRQ.store(irq, Ordering::Release);
    arm(reload);
    Ok(freq)
}

/// The frequency recorded by [`init`].
#[allow(dead_code)] // Diagnostic accessor for later platform tickets.
pub fn configured_frequency() -> u64 {
    FREQ.load(Ordering::Acquire)
}

/// The interrupt line recorded by [`init`].
#[allow(dead_code)] // Diagnostic accessor for later platform tickets.
pub fn interrupt() -> u32 {
    IRQ.load(Ordering::Acquire)
}

/// Timer interrupt handler: re-arm for the next period and account the tick.
pub fn handle() {
    let reload = RELOAD.load(Ordering::Acquire);
    if reload != 0 {
        arm(reload);
    }
    crate::time::on_tick();
}

fn arm(reload: u64) {
    // SAFETY: CNTP_TVAL_EL0/CNTP_CTL_EL0 are the EL1 physical timer; ENABLE=1, IMASK=0. The ISB
    // makes the new comparator value visible before the next interrupt can be taken.
    unsafe {
        asm!(
            "msr cntp_tval_el0, {}",
            "mov x9, #1",
            "msr cntp_ctl_el0, x9",
            "isb",
            in(reg) reload,
            out("x9") _,
            options(nostack, preserves_flags),
        );
    }
}
