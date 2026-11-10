// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-CPU accessors shared by the scheduler, `sync` and the architecture code.
//!
//! The GS block itself lives in `arch::x86_64::ap_start` because it is written by the trampoline
//! and by MSR setup. This module owns the per-CPU preemption depth, which must not be global: a
//! critical section on one CPU must not suppress preemption on another.

use core::sync::atomic::{AtomicU32, Ordering};

use super::MAX_CPUS;
use crate::arch::x86_64::ap_start;

static PREEMPT: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];

/// Index of the CPU executing this code.
#[inline]
pub fn this_cpu() -> usize {
    ap_start::this_cpu()
}

/// Install the BSP GS base pair (repairs the legacy `syscall::init` setup).
#[inline]
pub fn install_bsp_gs() {
    ap_start::install_gs(0);
}

/// Update the kernel stack pointer used by the syscall entry stub.
#[inline]
pub fn set_kernel_rsp(value: u64) {
    ap_start::set_kernel_rsp(value);
}

/// Record the current thread pointer in the GS block.
#[inline]
pub fn set_current_thread(value: u64) {
    ap_start::set_current_thread(value);
}

/// Current preemption nesting depth of this CPU.
#[inline]
pub fn preempt_count() -> u32 {
    PREEMPT
        .get(this_cpu())
        .map(|counter| counter.load(Ordering::SeqCst))
        .unwrap_or(0)
}

/// Install a thread's saved preemption depth on this CPU.
#[inline]
pub fn set_preempt_count(value: u32) {
    if let Some(counter) = PREEMPT.get(this_cpu()) {
        counter.store(value, Ordering::SeqCst);
    }
}

/// Enter one nesting level.
#[inline]
pub fn preempt_inc() {
    if let Some(counter) = PREEMPT.get(this_cpu()) {
        counter.fetch_add(1, Ordering::SeqCst);
    }
}

/// Leave one nesting level.
#[inline]
pub fn preempt_dec() {
    if let Some(counter) = PREEMPT.get(this_cpu()) {
        counter.fetch_sub(1, Ordering::SeqCst);
    }
}
