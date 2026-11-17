// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-CPU state for aarch64.
//!
//! `TPIDR_EL1` holds the pointer to the running CPU's [`PerCpuBlock`], matching the x86-64 GS
//! base. The block carries the CPU index, the current thread pointer and the kernel stack slot.
//! `this_cpu()` validates the pointer against the static array, so a zero or stale `TPIDR_EL1`
//! (before the BSP installs its block) safely resolves to CPU 0.

use core::arch::asm;
use core::mem::size_of;
use core::sync::atomic::{AtomicU64, Ordering};

/// Maximum CPUs in the per-CPU table; must match `sched::smp::MAX_CPUS`.
pub const MAX_CPUS: usize = 64;

/// One CPU's architectural block, reachable through `TPIDR_EL1`.
#[repr(C, align(64))]
pub struct PerCpuBlock {
    /// Pointer to this block (offset 0).
    pub self_ptr: AtomicU64,
    /// Kernel stack top for exception entry / diagnostics (offset 8).
    pub kernel_sp: AtomicU64,
    /// Current thread pointer (offset 16).
    pub current_thread: AtomicU64,
    /// CPU index (offset 24).
    pub cpu_index: AtomicU64,
    /// Set when the CPU has completed architecture setup (offset 32).
    pub online: AtomicU64,
    /// Reserved (offset 40).
    pub reserved: AtomicU64,
}

impl PerCpuBlock {
    const fn new() -> Self {
        Self {
            self_ptr: AtomicU64::new(0),
            kernel_sp: AtomicU64::new(0),
            current_thread: AtomicU64::new(0),
            cpu_index: AtomicU64::new(0),
            online: AtomicU64::new(0),
            reserved: AtomicU64::new(0),
        }
    }
}

const _: () = {
    assert!(size_of::<PerCpuBlock>() == 64);
    assert!(core::mem::offset_of!(PerCpuBlock, self_ptr) == 0);
    assert!(core::mem::offset_of!(PerCpuBlock, current_thread) == 16);
    assert!(core::mem::offset_of!(PerCpuBlock, cpu_index) == 24);
    assert!(core::mem::offset_of!(PerCpuBlock, online) == 32);
};

static BLOCKS: [PerCpuBlock; MAX_CPUS] = [const { PerCpuBlock::new() }; MAX_CPUS];

/// One CPU's block, clamped to the table.
#[must_use]
pub fn block(cpu: usize) -> &'static PerCpuBlock {
    BLOCKS
        .get(cpu.min(MAX_CPUS.saturating_sub(1)))
        .unwrap_or(&BLOCKS[0])
}

/// Virtual address of one CPU's block.
#[must_use]
pub fn address(cpu: usize) -> u64 {
    block(cpu) as *const PerCpuBlock as u64
}

/// Index of the CPU executing this code.
#[inline]
pub fn this_cpu() -> usize {
    let self_ptr = read_tpidr();
    let base = core::ptr::addr_of!(BLOCKS) as u64;
    let stride = size_of::<PerCpuBlock>() as u64;
    let end = base.saturating_add(stride.saturating_mul(MAX_CPUS as u64));
    if self_ptr < base || self_ptr >= end {
        return 0;
    }
    usize::try_from(
        self_ptr
            .saturating_sub(base)
            .checked_div(stride)
            .unwrap_or(0),
    )
    .unwrap_or(0)
    .min(MAX_CPUS.saturating_sub(1))
}

/// Install the per-CPU block for `cpu` and publish it in `TPIDR_EL1`.
pub fn install(cpu: usize) {
    let slot = block(cpu);
    slot.self_ptr.store(address(cpu), Ordering::Release);
    slot.cpu_index
        .store(u64::try_from(cpu).unwrap_or(0), Ordering::Release);
    // SAFETY: TPIDR_EL1 is an EL1 register used here as the per-CPU block pointer.
    unsafe {
        asm!("msr tpidr_el1, {}", in(reg) address(cpu), options(nomem, nostack));
    }
}

/// Mark the CPU online (called by the secondary entry after its architecture state is ready).
pub fn mark_online(cpu: usize) {
    block(cpu).online.store(1, Ordering::Release);
}

/// True once `cpu` has finished its secondary entry.
#[must_use]
pub fn is_online(cpu: usize) -> bool {
    block(cpu).online.load(Ordering::Acquire) != 0
}

/// Record the current thread pointer on this CPU.
pub fn set_current_thread(value: u64) {
    block(this_cpu())
        .current_thread
        .store(value, Ordering::Release);
}

/// Record the kernel stack top on this CPU.
pub fn set_kernel_sp(value: u64) {
    block(this_cpu()).kernel_sp.store(value, Ordering::Release);
}

fn read_tpidr() -> u64 {
    let value: u64;
    // SAFETY: TPIDR_EL1 is readable at EL1 and holds only our block pointer or zero.
    unsafe {
        asm!("mrs {}, tpidr_el1", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}
