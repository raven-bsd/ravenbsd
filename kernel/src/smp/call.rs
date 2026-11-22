// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cross-CPU probe mailbox used by the SMP self-test.
//!
//! One CPU at a time publishes a virtual address, IPIs the target and waits for the target to
//! read the byte. The handler runs with interrupts disabled, so the probe must stay short.
//!
//! # Locking and timeout discipline
//!
//! One active probe is protected by [`ActiveGuard`]. A second shooter waits with interrupts
//! enabled, so it can take the first shooter's probe IPI; the acknowledgement wait also runs
//! with interrupts enabled. The target's handler only touches this module's atomics and reads
//! one byte. A missing acknowledgement is a 100 ms timeout: the per-CPU pending slot is cleared,
//! the failure is logged, and `None` is returned. No frame or slot is allocated, so a timeout
//! cannot leak, and a late IPI sees an idle mailbox.
//!
//! Every index is bounded by `online_cpus() <= MAX_CPUS`, so the fixed per-CPU arrays are safe.

#![allow(clippy::indexing_slicing)]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::arch::x86_64::{ap_start, ipi};
use crate::kprintln;
use crate::sched::smp::percpu;
use crate::time;

use super::MAX_CPUS;

/// One active probe. The shooter waits for it with interrupts enabled.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Serialises probes without masking interrupts while waiting for the active slot.
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
            // A waiter must be able to take the active probe's IPI.
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
static ADDRESS: AtomicU64 = AtomicU64::new(0);
static VALUE: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(u64::MAX) }; MAX_CPUS];
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PENDING: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static ACKED: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];

/// Read one byte at `address` on `cpu`, returning `None` on timeout.
pub fn probe(cpu: usize, address: u64) -> Option<u8> {
    if cpu >= super::online_cpus() {
        return None;
    }
    if cpu == percpu::this_cpu() {
        // SAFETY: the self-test maps every probe address before calling this function.
        return Some(unsafe { ap_start::probe_read(address) });
    }

    let _guard = ActiveGuard::acquire();
    let sequence = SEQUENCE.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    ADDRESS.store(address, Ordering::SeqCst);
    VALUE[cpu].store(u64::MAX, Ordering::SeqCst);
    ACKED[cpu].store(0, Ordering::SeqCst);
    PENDING[cpu].store(sequence, Ordering::SeqCst);
    ipi::send_call(cpu);

    let deadline = time::now_ns().saturating_add(100_000_000);
    // The target may itself be waiting for the active slot or an IPI; wait with interrupts
    // enabled so it can take this probe. The guard restores the caller's original IF.
    ap_start::enable_local_interrupts();
    while ACKED[cpu].load(Ordering::SeqCst) != sequence {
        if time::now_ns() > deadline {
            // Idle the mailbox so a late IPI is ignored; the next probe uses a new sequence.
            PENDING[cpu].store(0, Ordering::SeqCst);
            ap_start::disable_local_interrupts();
            kprintln!("smp: call probe timeout on cpu {cpu}");
            return None;
        }
        core::hint::spin_loop();
    }
    ap_start::disable_local_interrupts();
    let value = VALUE[cpu].load(Ordering::SeqCst);
    if value == u64::MAX {
        None
    } else {
        Some(value as u8)
    }
}

/// Handle a probe IPI on the current CPU.
pub fn handle() {
    let cpu = percpu::this_cpu();
    let sequence = PENDING[cpu].load(Ordering::SeqCst);
    if sequence == 0 || ACKED[cpu].load(Ordering::SeqCst) == sequence {
        return;
    }
    let address = ADDRESS.load(Ordering::SeqCst);
    // SAFETY: only the self-test publishes probe addresses, and it maps them first.
    let value = unsafe { ap_start::probe_read(address) };
    VALUE[cpu].store(u64::from(value), Ordering::SeqCst);
    ACKED[cpu].store(sequence, Ordering::SeqCst);
}
