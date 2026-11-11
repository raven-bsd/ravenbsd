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
//! * `LOCK` is an [`IrqSpinLock`]: entry masks interrupts on the shooting CPU.  The wait loop
//!   re-enables them explicitly (and disables them again before returning) so a target that is
//!   spinning on an address-space lock can still take the IPI.  On return the guard restores the
//!   caller's original IF.
//! * A missing acknowledgement is a timeout after 100 ms.  The per-CPU pending slot is cleared so
//!   a late IPI is ignored, the timeout is logged, and `false` is returned; the caller decides how
//!   to react.  No frame or slot is allocated, so a timeout cannot leak.
//!
//! Memory ordering: the shooter stores the range and the per-CPU pending sequence with
//! `SeqCst` before sending the IPI; the handler loads the pending sequence and the range with
//! `SeqCst`, executes `invlpg`, then stores the acknowledgement with `SeqCst`. The shooter's
//! acknowledgement load is `SeqCst`, so the flush happens before the page tables are reused.
//!
//! # Fault injection
//!
//! The `s34-tlb-fail` boot flag arms a one-shot injection: the next shootdown skips one remote
//! IPI, waits out the normal deadline, logs the timeout and returns `false`.  It exists to test
//! the timeout path under SMP load; it never fires unless the flag is present.

#![allow(clippy::indexing_slicing)]

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::arch::x86_64::{ap_start, ipi};
use crate::kprintln;
use crate::sched::smp::percpu;
use crate::sync::IrqSpinLock;
use crate::time;

use super::MAX_CPUS;

static LOCK: IrqSpinLock<()> = IrqSpinLock::new(());
/// One-shot TLB failure injection (`s34-tlb-fail`): skip one remote IPI.
static INJECT_FAILURE: AtomicUsize = AtomicUsize::new(0);
static ADDRESS: AtomicU64 = AtomicU64::new(0);
static LENGTH: AtomicU64 = AtomicU64::new(0);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PENDING: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static ACKED: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static LAST: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static FLUSHES: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];

/// Arm the one-shot timeout injection used by the `concurrency-stress` test.
pub fn set_failure_injection(enabled: bool) {
    INJECT_FAILURE.store(usize::from(enabled), Ordering::Release);
}

/// Invalidate `addr..addr + len` on every online CPU.
///
/// The local CPU is flushed unconditionally; remote CPUs receive one IPI and acknowledge after
/// executing the invalidation. A missing acknowledgement is reported and does not hang the
/// system forever.
pub fn flush_range(addr: u64, len: u64) -> bool {
    if len == 0 {
        return true;
    }
    let me = percpu::this_cpu();
    let online = super::online_cpus();
    ap_start::flush_range_local(addr, len);
    if online <= 1 {
        return true;
    }

    let _guard = LOCK.lock();
    let inject = INJECT_FAILURE.swap(0, Ordering::AcqRel) != 0;
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
            } else {
                ipi::send_tlb(cpu);
            }
        }
        cpu = cpu.saturating_add(1);
    }

    // The caller may be inside a syscall or page fault with IF clear. Re-enable interrupts only
    // while waiting so a target that is spinning on the address-space lock can still take the
    // IPI. The outer `IrqSpinLock` restores the original IF when this function returns.
    let deadline = time::now_ns().saturating_add(100_000_000);
    ap_start::enable_local_interrupts();
    let mut all_acked = true;
    cpu = 0;
    while cpu < online {
        if cpu != me {
            while ACKED[cpu].load(Ordering::SeqCst) != sequence {
                if time::now_ns() > deadline {
                    // Reset the mailbox so a late IPI is ignored instead of acknowledging a
                    // stale sequence; the next shootdown publishes a fresh one.
                    PENDING[cpu].store(0, Ordering::SeqCst);
                    kprintln!("smp: tlb shootdown timeout on cpu {cpu}");
                    all_acked = false;
                    break;
                }
                core::hint::spin_loop();
            }
        }
        cpu = cpu.saturating_add(1);
    }
    ap_start::disable_local_interrupts();
    all_acked
}

/// Handle a TLB IPI on the current CPU.
pub fn handle_ipi() {
    let cpu = percpu::this_cpu();
    let sequence = PENDING[cpu].load(Ordering::SeqCst);
    if sequence == 0 || LAST[cpu].load(Ordering::SeqCst) == sequence {
        return;
    }
    let addr = ADDRESS.load(Ordering::SeqCst);
    let len = LENGTH.load(Ordering::SeqCst);
    ap_start::flush_range_local(addr, len);
    FLUSHES[cpu].fetch_add(1, Ordering::SeqCst);
    LAST[cpu].store(sequence, Ordering::SeqCst);
    ACKED[cpu].store(sequence, Ordering::SeqCst);
}

/// Number of TLB IPIs handled by `cpu` (self-test diagnostics).
pub fn flush_count(cpu: usize) -> u64 {
    FLUSHES
        .get(cpu)
        .map(|counter| counter.load(Ordering::Relaxed))
        .unwrap_or(0)
}
