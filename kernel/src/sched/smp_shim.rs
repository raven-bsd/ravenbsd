// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Single-CPU stand-in for the x86-64 SMP module.
//!
//! S14b runs aarch64 on the boot processor only; PSCI SMP is a later ticket. The scheduler and
//! its self-tests use `sched::smp`, so this module provides the same small surface: one online
//! CPU, per-CPU preemption depth, the current-thread pointer in the architecture block, and no-op
//! reschedule IPIs.

/// Maximum number of CPUs in the per-CPU tables.
pub const MAX_CPUS: usize = 1;

/// Number of CPUs currently online.
pub fn online_cpus() -> usize {
    1
}

/// Publish the number of CPUs that completed bring-up (always one here).
pub(crate) fn set_online(_count: usize) {}

/// Per-CPU accessors used by the scheduler, `sync` and the self-tests.
pub mod percpu {
    use core::sync::atomic::{AtomicU32, Ordering};

    use crate::arch::percpu;

    static PREEMPT: AtomicU32 = AtomicU32::new(0);

    /// Index of the CPU executing this code.
    #[inline]
    pub fn this_cpu() -> usize {
        percpu::this_cpu()
    }

    /// Current preemption nesting depth.
    #[inline]
    pub fn preempt_count() -> u32 {
        PREEMPT.load(Ordering::SeqCst)
    }

    /// Install a thread's saved preemption depth.
    #[inline]
    pub fn set_preempt_count(value: u32) {
        PREEMPT.store(value, Ordering::SeqCst);
    }

    /// Enter one nesting level.
    #[inline]
    pub fn preempt_inc() {
        PREEMPT.fetch_add(1, Ordering::SeqCst);
    }

    /// Leave one nesting level.
    #[inline]
    pub fn preempt_dec() {
        PREEMPT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Cross-CPU services are no-ops on a single CPU.
pub mod bringup {
    /// Request a reschedule on another CPU.
    pub fn send_reschedule(_cpu: usize) {}
}

/// Single-CPU TLB shootdown stand-in.
///
/// There is no other CPU to notify, so a "shootdown" is one local flush. The boolean result keeps
/// the shared caller contract: when false, the caller must not recycle frames.
pub mod shootdown {
    /// Flush the translation for `[start, start + len)` after a mapping change.
    pub fn flush_range(_start: u64, _len: u64) -> bool {
        crate::arch::mmu::tlb_flush_all();
        true
    }
}
