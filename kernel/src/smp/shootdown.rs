// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cross-CPU TLB shootdown.
//!
//! The S24 address-space code calls [`flush_range`] after changing user page tables. The local CPU
//! has already executed `invlpg` through the mapper; this module publishes the range, sends the
//! TLB IPI to every other online CPU and waits for each acknowledgement. A single global sequence
//! number serialises shootdowns.
//!
//! Every index is bounded by `online_cpus() <= MAX_CPUS`, so the fixed per-CPU arrays are safe.
//!
//! # Locking and interrupt contract
//!
//! * The caller may hold any address-space lock, but must not hold the scheduler's per-CPU slot
//!   lock or the wait-queue lock: the IPI handler only touches this module's atomics and executes
//!   `invlpg`, so it has no dependency on those.
//! * One active request is protected by [`ActiveGuard`], not an IRQ-masked spinlock. A second
//!   shooter waits with interrupts enabled, so a target that is spinning on an address-space
//!   lock can still take the TLB IPI. The guard masks interrupts only while publishing/clearing
//!   the active request and restores the caller's original IF on drop.
//! * A missing acknowledgement is a timeout after 100 ms.  The per-CPU pending slot is cleared so
//!   a late IPI is ignored, the timeout is logged, and `false` is returned; the caller decides how
//!   to react.  No frame or slot is allocated, so a timeout cannot leak.
//!
//! Memory ordering: the shooter stores the range and the per-CPU pending sequence with
//! `SeqCst` before sending the IPI; the handler loads the pending sequence and the range with
//! `SeqCst`, executes `invlpg`, then stores the acknowledgement with `SeqCst`. The shooter's
//! acknowledgement/flush-generation load is `SeqCst`, so the flush happens before the page
//! tables are reused. The handler publishes `LAST` immediately after the invalidation and the
//! explicit `ACKED` after it; the shooter accepts either as completion.
//!
//! # Fault injection
//!
//! The `s34-tlb-fail` boot flag arms a one-shot injection: the next shootdown skips one remote
//! IPI, waits out the normal deadline, logs the timeout and returns `false`.  It exists to test
//! the timeout path under SMP load; it never fires unless the flag is present.

#![allow(clippy::indexing_slicing)]

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::arch::x86_64::{ap_start, ipi};
use crate::kprintln;
use crate::sched::smp::percpu;
use crate::time;

use super::MAX_CPUS;

/// One active shootdown. The shooter waits for it with interrupts enabled.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Serialises shootdowns without masking interrupts while waiting for the active slot.
struct ActiveGuard {
    flags: u64,
}

impl ActiveGuard {
    fn acquire() -> Self {
        let flags = ap_start::save_and_disable_local_interrupts();
        percpu::preempt_inc();
        loop {
            if !ACTIVE.swap(true, Ordering::Acquire) {
                break;
            }
            // A waiter must be able to take the active request's TLB IPI.
            ap_start::enable_local_interrupts();
            while ACTIVE.load(Ordering::Acquire) {
                core::hint::spin_loop();
            }
            ap_start::disable_local_interrupts();
        }
        Self { flags }
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
        percpu::preempt_dec();
        ap_start::restore_local_interrupts(self.flags);
    }
}
/// One-shot TLB failure injection (`s34-tlb-fail`): skip one remote IPI.
static INJECT_FAILURE: AtomicUsize = AtomicUsize::new(0);
static ADDRESS: AtomicU64 = AtomicU64::new(0);
static LENGTH: AtomicU64 = AtomicU64::new(0);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PENDING: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static ACKED: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static LAST: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static LAST_TIME: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static FLUSHES: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
/// Selftest hook: skip the NMI for one target, leaving the fixed IPI as the only delivery.
static DROP_NMI: [AtomicBool; MAX_CPUS] = [const { AtomicBool::new(false) }; MAX_CPUS];
/// Selftest hook: delay one target's acknowledgement inside the NMI handler.
static ACK_DELAY_NS: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];

/// Arm the one-shot timeout injection used by the `concurrency-stress` test.
pub fn set_failure_injection(enabled: bool) {
    INJECT_FAILURE.store(usize::from(enabled), Ordering::Release);
}

/// Selftest hook: drop the NMI delivery to `cpu` once, so only the fixed IPI can complete it.
pub fn arm_nmi_drop(cpu: usize) {
    if let Some(slot) = DROP_NMI.get(cpu) {
        slot.store(true, Ordering::Release);
    }
}

/// Selftest hook: delay `cpu`'s acknowledgement by `delay_ns` inside the next handler.
pub fn arm_ack_delay(cpu: usize, delay_ns: u64) {
    if let Some(slot) = ACK_DELAY_NS.get(cpu) {
        slot.store(delay_ns, Ordering::Release);
    }
}

/// The pending sequence for `cpu`, or zero when no request is published.
pub fn pending_sequence(cpu: usize) -> u64 {
    PENDING
        .get(cpu)
        .map(|slot| slot.load(Ordering::SeqCst))
        .unwrap_or(0)
}

/// Invalidate `addr..addr + len` on every online CPU.
///
/// The local CPU is flushed unconditionally; remote CPUs receive one IPI and acknowledge after
/// executing the invalidation. A missing acknowledgement is reported and does not hang the
/// system forever.
pub fn flush_range(addr: u64, len: u64) -> bool {
    debug_assert!(
        !percpu::unmap_lock_held(),
        "TLB shootdown while holding the address-space lock"
    );
    if len == 0 {
        return true;
    }
    let me = percpu::this_cpu();
    let online = super::online_cpus();
    ap_start::flush_range_local(addr, len);
    if online <= 1 {
        return true;
    }

    let _guard = ActiveGuard::acquire();
    let inject = INJECT_FAILURE.swap(0, Ordering::AcqRel) != 0;
    let start = time::now_ns();
    let sequence = SEQUENCE.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    ADDRESS.store(addr, Ordering::SeqCst);
    LENGTH.store(len, Ordering::SeqCst);

    let mut cpu = 0usize;
    while cpu < online {
        if cpu != me {
            ACKED[cpu].store(0, Ordering::SeqCst);
            PENDING[cpu].store(sequence, Ordering::SeqCst);
        }
        cpu = cpu.saturating_add(1);
    }
    let mut skipped = false;
    cpu = 0;
    while cpu < online {
        if cpu != me {
            if inject && !skipped {
                skipped = true;
            } else if !DROP_NMI[cpu].swap(false, Ordering::AcqRel) {
                ipi::send_tlb(cpu);
            }
        }
        cpu = cpu.saturating_add(1);
    }

    // The caller may be inside a syscall or page fault with IF clear. Re-enable interrupts only
    // while waiting so a target that is spinning on the address-space lock can still take the
    // IPI. `ActiveGuard::drop` restores the caller's original IF.
    let deadline = start.saturating_add(100_000_000);
    ap_start::enable_local_interrupts();
    let mut all_acked = true;
    cpu = 0;
    while cpu < online {
        if cpu != me {
            while ACKED[cpu].load(Ordering::SeqCst) != sequence
                && LAST[cpu].load(Ordering::SeqCst) != sequence
            {
                if time::now_ns() > deadline {
                    // Test-only baseline: the pre-fix bounded timeout.
                    report_stall("timeout", cpu, sequence, start);
                    PENDING[cpu].store(0, Ordering::SeqCst);
                    all_acked = false;
                    break;
                }
                core::hint::spin_loop();
            }
        }
        if !all_acked {
            break;
        }
        cpu = cpu.saturating_add(1);
    }
    ap_start::disable_local_interrupts();
    all_acked
}

/// Print the bounded diagnostic for a target that has not acknowledged within 100 ms.
fn report_stall(kind: &str, cpu: usize, sequence: u64, start: u64) {
    let (state, last_irq, last_state) = percpu::snapshot(cpu);
    let (in_syscall, user_flags, syscall_number) = percpu::syscall_state(cpu);
    let page_fault = percpu::page_fault_active(cpu);
    let page_fault_phase = percpu::page_fault_phase(cpu);
    let wait_phase = percpu::wait_phase(cpu);
    let now = time::now_ns();
    let ack_time = LAST_TIME[cpu].load(Ordering::SeqCst);
    kprintln!(
        "smp: tlb shootdown {kind} on cpu {cpu} seq={sequence} last={} flushes={} state={state} syscall={in_syscall} pf={page_fault} pfp={page_fault_phase} wq={wait_phase} nr={syscall_number:#x} user_if={} irq_age_ms={} state_age_ms={} ack_after_ms={} ack_age_ms={}",
        LAST[cpu].load(Ordering::SeqCst),
        flush_count(cpu),
        (user_flags >> 9) & 1,
        now.saturating_sub(last_irq) / 1_000_000,
        now.saturating_sub(last_state) / 1_000_000,
        if ack_time >= start {
            ack_time.saturating_sub(start) / 1_000_000
        } else {
            u64::MAX
        },
        now.saturating_sub(ack_time) / 1_000_000
    );
}

/// Handle a TLB IPI on the current CPU.
///
/// Returns true when a pending shootdown was found and acknowledged. The NMI handler uses the
/// result to distinguish a shootdown from an unexpected non-maskable interrupt.
pub fn handle_ipi() -> bool {
    let cpu = percpu::this_cpu();
    let delay = ACK_DELAY_NS[cpu].swap(0, Ordering::AcqRel);
    if delay != 0 {
        time::delay_ns(delay);
    }
    let sequence = PENDING[cpu].load(Ordering::SeqCst);
    if sequence == 0 || LAST[cpu].load(Ordering::SeqCst) == sequence {
        return false;
    }
    let addr = ADDRESS.load(Ordering::SeqCst);
    let len = LENGTH.load(Ordering::SeqCst);
    ap_start::flush_range_local(addr, len);
    FLUSHES[cpu].fetch_add(1, Ordering::SeqCst);
    LAST[cpu].store(sequence, Ordering::SeqCst);
    LAST_TIME[cpu].store(time::now_ns(), Ordering::SeqCst);
    ACKED[cpu].store(sequence, Ordering::SeqCst);
    true
}

/// Number of TLB IPIs handled by `cpu` (self-test diagnostics).
pub fn flush_count(cpu: usize) -> u64 {
    FLUSHES
        .get(cpu)
        .map(|counter| counter.load(Ordering::Relaxed))
        .unwrap_or(0)
}
