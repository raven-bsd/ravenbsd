// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cross-CPU probe mailbox used by the SMP self-test.
//!
//! One CPU at a time publishes a virtual address, IPIs the target and waits for the target to
//! read the byte. The handler runs with interrupts disabled, so the probe must stay short.
//!
//! Every index is bounded by `online_cpus() <= MAX_CPUS`, so the fixed per-CPU arrays are safe.

#![allow(clippy::indexing_slicing)]

use core::sync::atomic::{AtomicU64, Ordering};

use crate::arch::x86_64::{ap_start, ipi};
use crate::sched::smp::percpu;
use crate::sync::IrqSpinLock;
use crate::time;

use super::MAX_CPUS;

static LOCK: IrqSpinLock<()> = IrqSpinLock::new(());
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

    let _guard = LOCK.lock();
    let sequence = SEQUENCE.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    ADDRESS.store(address, Ordering::SeqCst);
    VALUE[cpu].store(u64::MAX, Ordering::SeqCst);
    ACKED[cpu].store(0, Ordering::SeqCst);
    PENDING[cpu].store(sequence, Ordering::SeqCst);
    ipi::send_call(cpu);

    let deadline = time::now_ns().saturating_add(100_000_000);
    while ACKED[cpu].load(Ordering::SeqCst) != sequence {
        if time::now_ns() > deadline {
            return None;
        }
        core::hint::spin_loop();
    }
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
