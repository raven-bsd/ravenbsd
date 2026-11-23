// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cross-CPU call queue.
//!
//! The DragonFlyBSD `lwkt_ipiq` FIFO (`crates/raven-ipiq`) is the live delivery path: each target
//! CPU owns one FIFO, producers serialise on a short spin mutex, and the IPI handler drains the
//! queue. `probe` keeps its single-outstanding-probe acknowledgement contract; `batch` exercises
//! the queue with many independent calls in one IPI.
//!
//! # Locking and timeout discipline
//!
//! One probe is still protected by [`ActiveGuard`] because the acknowledgement slot is per target
//! and one probe is outstanding. The queue producer mutex is held only for the index update.
//! The target's handler runs with interrupts disabled and never allocates. A missing
//! acknowledgement is a 100 ms timeout; the queue entry is already consumed by the target's
//! drain, so a late IPI sees an empty queue.

#![allow(clippy::indexing_slicing)]

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use raven_ipiq::{IpIQ, Message};

use crate::arch::x86_64::{ap_start, ipi};
use crate::kprintln;
use crate::sched::smp::percpu;
use crate::time;

use super::MAX_CPUS;

/// One queued call. `Probe` reads one user-mapped byte; `Batch` records one completion bit.
#[derive(Clone, Copy)]
enum CallOp {
    Probe { address: u64, sequence: u64 },
    Batch(u32),
}

/// One target CPU's FIFO plus its producer serialisation.
struct CallQueue {
    producer: spin::Mutex<()>,
    queue: IpIQ<CallOp>,
}

impl CallQueue {
    const fn new() -> Self {
        Self {
            producer: spin::Mutex::new(()),
            queue: IpIQ::new(),
        }
    }
}

static QUEUES: [CallQueue; MAX_CPUS] = [const { CallQueue::new() }; MAX_CPUS];

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

static VALUE: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(u64::MAX) }; MAX_CPUS];
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static ACKED: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static BATCH_DONE: AtomicU64 = AtomicU64::new(0);

fn enqueue(cpu: usize, op: CallOp) {
    let queue = &QUEUES[cpu];
    let _guard = queue.producer.lock();
    // The queue is 256 entries and the caller may exceed it; drain locally if full.
    if queue
        .queue
        .push(Message {
            func: op,
            arg1: 0,
            arg2: 0,
        })
        .is_err()
    {
        // The target will drain; drop this request rather than spinning in the caller.
    }
}

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
    VALUE[cpu].store(u64::MAX, Ordering::SeqCst);
    ACKED[cpu].store(0, Ordering::SeqCst);
    enqueue(cpu, CallOp::Probe { address, sequence });
    ipi::send_call(cpu);

    let deadline = time::now_ns().saturating_add(100_000_000);
    // The target may itself be waiting for the active slot or an IPI; wait with interrupts
    // enabled so it can take this probe. The guard restores the caller's original IF.
    ap_start::enable_local_interrupts();
    while ACKED[cpu].load(Ordering::SeqCst) != sequence {
        if time::now_ns() > deadline {
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

/// Send `count` independent calls to `cpu` in one IPI and wait for every completion.
///
/// Returns `(completed, elapsed_ns)`. The FIFO carries all calls; the old single-slot mailbox
/// could carry one, which is the regression this row guards.
pub fn batch(cpu: usize, count: u32) -> (u32, u64) {
    if cpu >= super::online_cpus() || cpu == percpu::this_cpu() || count == 0 || count > 64 {
        return (0, 0);
    }
    BATCH_DONE.store(0, Ordering::SeqCst);
    for value in 0..count {
        enqueue(cpu, CallOp::Batch(value));
    }
    let started = time::now_ns();
    ipi::send_call(cpu);

    let deadline = started.saturating_add(500_000_000);
    ap_start::enable_local_interrupts();
    while BATCH_DONE.load(Ordering::SeqCst).count_ones() != count {
        if time::now_ns() > deadline {
            break;
        }
        core::hint::spin_loop();
    }
    ap_start::disable_local_interrupts();
    let elapsed = time::now_ns().saturating_sub(started);
    (BATCH_DONE.load(Ordering::SeqCst).count_ones(), elapsed)
}

/// Number of calls queued to `cpu` but not yet processed (evidence accessor).
pub fn pending(cpu: usize) -> u32 {
    QUEUES.get(cpu).map_or(0, |queue| queue.queue.pending())
}

/// Handle a call IPI on the current CPU: drain the whole FIFO.
pub fn handle() {
    let cpu = percpu::this_cpu();
    let Some(queue) = QUEUES.get(cpu) else {
        return;
    };
    let _ = queue.queue.process(0, |message| match message.func {
        CallOp::Probe { address, sequence } => {
            // SAFETY: only the self-test publishes probe addresses, and it maps them first.
            let value = unsafe { ap_start::probe_read(address) };
            VALUE[cpu].store(u64::from(value), Ordering::SeqCst);
            ACKED[cpu].store(sequence, Ordering::SeqCst);
        }
        CallOp::Batch(value) => {
            BATCH_DONE.fetch_or(1u64 << value, Ordering::SeqCst);
        }
    });
}
