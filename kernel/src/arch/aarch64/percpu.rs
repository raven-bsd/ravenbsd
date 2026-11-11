// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Single-CPU per-CPU state for aarch64.
//!
//! `TPIDR_EL1` holds the current [`crate::sched::thread::Thread`] pointer; the kernel keeps the
//! same value in a static so the context switch and diagnostics agree. PSCI SMP will replace the
//! static with a real per-CPU block in a later ticket.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

static CURRENT_THREAD: AtomicU64 = AtomicU64::new(0);

/// Index of the CPU executing this code (always zero before PSCI SMP).
pub fn this_cpu() -> usize {
    0
}

/// The current thread pointer recorded by the scheduler.
#[allow(dead_code)] // Diagnostic accessor; the scheduler reads its own per-CPU slot.
pub fn current_thread() -> u64 {
    CURRENT_THREAD.load(Ordering::Acquire)
}

/// Publish the current thread in the static and in `TPIDR_EL1`.
pub fn set_current_thread(value: u64) {
    CURRENT_THREAD.store(value, Ordering::Release);
    // SAFETY: TPIDR_EL1 is an EL1 system register used here only as a thread pointer slot.
    unsafe {
        asm!("msr tpidr_el1, {}", in(reg) value, options(nomem, nostack));
    }
}
