// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-CPU accessors shared by the scheduler, `sync` and the architecture code.
//!
//! The GS block itself lives in `arch::x86_64::ap_start` because it is written by the trampoline
//! and by MSR setup. This module owns the per-CPU preemption depth, which must not be global: a
//! critical section on one CPU must not suppress preemption on another.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use super::MAX_CPUS;
use crate::arch::x86_64::ap_start;

static PREEMPT: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];

/// Diagnostic target state: 0 = kernel, 1 = user, 2 = idle.
static TARGET_STATE: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
/// Time of the last timer interrupt on the CPU.
static LAST_IRQ: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
/// Time of the last scheduler/user/idle state change.
static LAST_STATE: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
/// One while the CPU is inside a syscall (the syscall entry clears IF).
static IN_SYSCALL: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
/// Last user RFLAGS observed at the syscall boundary or ring-3 entry.
static USER_FLAGS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
/// One while the CPU is servicing a user page fault (interrupt gate, IF clear).
static IN_PAGE_FAULT: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
/// Last completed page-fault stage (diagnostic).
static PAGE_FAULT_PHASE: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
/// Last wait-queue stage reached by the CPU (diagnostic).
static WAIT_PHASE: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
/// One while this CPU holds an address-space mutex that may need a TLB shootdown.
static UNMAP_LOCK_HELD: [AtomicBool; MAX_CPUS] = [const { AtomicBool::new(false) }; MAX_CPUS];
/// Syscall number currently executing (diagnostic).
static SYSCALL_NUMBER: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];

/// Note that the timer ISR ran on this CPU (diagnostic for a missed TLB IPI).
pub fn note_irq() {
    let cpu = this_cpu();
    if let Some(slot) = LAST_IRQ.get(cpu) {
        slot.store(crate::time::now_ns(), Ordering::Relaxed);
    }
}

/// Note that the scheduler switched to a user or kernel thread on this CPU.
pub fn note_switch(user: bool) {
    let cpu = this_cpu();
    if let Some(slot) = TARGET_STATE.get(cpu) {
        slot.store(u64::from(user), Ordering::Relaxed);
    }
    if let Some(slot) = LAST_STATE.get(cpu) {
        slot.store(crate::time::now_ns(), Ordering::Relaxed);
    }
}

/// Note that this CPU entered the idle loop.
pub fn note_idle() {
    let cpu = this_cpu();
    if let Some(slot) = TARGET_STATE.get(cpu) {
        slot.store(2, Ordering::Relaxed);
    }
    if let Some(slot) = LAST_STATE.get(cpu) {
        slot.store(crate::time::now_ns(), Ordering::Relaxed);
    }
}

/// Note that the CPU entered or left a syscall.
pub fn note_syscall(active: bool) {
    let cpu = this_cpu();
    if let Some(slot) = IN_SYSCALL.get(cpu) {
        slot.store(u64::from(active), Ordering::Relaxed);
    }
}

/// Record the last user RFLAGS seen at a kernel/user boundary.
pub fn note_syscall_number(number: u64) {
    let cpu = this_cpu();
    if let Some(slot) = SYSCALL_NUMBER.get(cpu) {
        slot.store(number, Ordering::Relaxed);
    }
}

/// Record the last user RFLAGS seen at a kernel/user boundary.
pub fn note_user_flags(flags: u64) {
    let cpu = this_cpu();
    if let Some(slot) = USER_FLAGS.get(cpu) {
        slot.store(flags, Ordering::Relaxed);
    }
}

/// Snapshot of `(in_syscall, user_flags, syscall_number)` for `cpu`.
pub fn syscall_state(cpu: usize) -> (u64, u64, u64) {
    (
        IN_SYSCALL
            .get(cpu)
            .map(|slot| slot.load(Ordering::Relaxed))
            .unwrap_or(0),
        USER_FLAGS
            .get(cpu)
            .map(|slot| slot.load(Ordering::Relaxed))
            .unwrap_or(0),
        SYSCALL_NUMBER
            .get(cpu)
            .map(|slot| slot.load(Ordering::Relaxed))
            .unwrap_or(0),
    )
}

/// Note that this CPU entered or left a user page-fault handler.
pub fn note_page_fault(active: bool) {
    let cpu = this_cpu();
    if let Some(slot) = IN_PAGE_FAULT.get(cpu) {
        slot.store(u64::from(active), Ordering::Relaxed);
    }
}

/// Record the page-fault stage reached by this CPU.
pub fn note_page_fault_phase(phase: u64) {
    let cpu = this_cpu();
    if let Some(slot) = PAGE_FAULT_PHASE.get(cpu) {
        slot.store(phase, Ordering::Relaxed);
    }
}

/// Snapshot of the last page-fault stage reached by `cpu`.
pub fn page_fault_phase(cpu: usize) -> u64 {
    PAGE_FAULT_PHASE
        .get(cpu)
        .map(|slot| slot.load(Ordering::Relaxed))
        .unwrap_or(0)
}

/// Record the wait-queue stage reached by this CPU.
pub fn note_wait_phase(phase: u64) {
    let cpu = this_cpu();
    if let Some(slot) = WAIT_PHASE.get(cpu) {
        slot.store(phase, Ordering::Relaxed);
    }
}

/// Mark whether this CPU holds an address-space mutex across a potential TLB shootdown.
pub fn note_unmap_lock_held(active: bool) {
    let cpu = this_cpu();
    if let Some(slot) = UNMAP_LOCK_HELD.get(cpu) {
        slot.store(active, Ordering::Relaxed);
    }
}

/// True when this CPU holds an address-space mutex across a potential TLB shootdown.
pub fn unmap_lock_held() -> bool {
    UNMAP_LOCK_HELD
        .get(this_cpu())
        .is_some_and(|slot| slot.load(Ordering::Relaxed))
}

/// Snapshot of the last wait-queue stage reached by `cpu`.
pub fn wait_phase(cpu: usize) -> u64 {
    WAIT_PHASE
        .get(cpu)
        .map(|slot| slot.load(Ordering::Relaxed))
        .unwrap_or(0)
}

/// Snapshot of whether `cpu` is inside a page-fault handler.
pub fn page_fault_active(cpu: usize) -> u64 {
    IN_PAGE_FAULT
        .get(cpu)
        .map(|slot| slot.load(Ordering::Relaxed))
        .unwrap_or(0)
}

/// Snapshot of `(state, last_irq_ns, last_state_ns)` for `cpu`.
pub fn snapshot(cpu: usize) -> (u64, u64, u64) {
    (
        TARGET_STATE
            .get(cpu)
            .map(|slot| slot.load(Ordering::Relaxed))
            .unwrap_or(0),
        LAST_IRQ
            .get(cpu)
            .map(|slot| slot.load(Ordering::Relaxed))
            .unwrap_or(0),
        LAST_STATE
            .get(cpu)
            .map(|slot| slot.load(Ordering::Relaxed))
            .unwrap_or(0),
    )
}

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
