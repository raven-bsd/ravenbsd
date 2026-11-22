// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! DTrace kernel provider: probe state, per-CPU rings and the syscall probe.
//!
//! The pure registry and ring model lives in `crates/raven-dtrace-core` and is included here with
//! the same `#[path]` pattern as `raven-evq-core`, so the host tests and the kernel use one
//! implementation. The provider is clean-room: the Illumos `sys/dtrace_impl.h` notes (per-CPU
//! buffers, ring wrap, per-buffer drop accounting) informed the design; no Illumos code was
//! copied.

#[path = "../../../crates/raven-dtrace-core/src/lib.rs"]
pub(crate) mod model;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};

use raven_abi::errno::Errno;
use spin::Mutex;

use crate::event::UserEvent;
use crate::syscall::SyscallFrame;
use crate::syscall::dtrace::is_control;

/// Number of CPUs with a ring. Records produced on a higher CPU are counted as drops.
pub const MAX_DTRACE_CPUS: usize = 8;
/// Records per CPU ring.
pub const RING_CAPACITY: usize = 4096;

/// Bit of the syscall-entry probe in the enabled mask.
const SYSCALL_BIT: u64 = 1 << model::probe::SYSCALL_ENTRY_ID;

/// Per-CPU rings. The producer is the owning CPU; the consumer is serialised by `CONSUMER`.
static RINGS: [model::Ring<RING_CAPACITY>; MAX_DTRACE_CPUS] =
    [const { model::Ring::new() }; MAX_DTRACE_CPUS];

/// Enabled probe mask.
static ENABLED: AtomicU64 = AtomicU64::new(0);
/// Event signalled when a ring transitions from empty to non-empty.
static WAKE_EVENT: Mutex<Option<Arc<UserEvent>>> = Mutex::new(None);
/// Serialises `DTRACE_READ` and `DTRACE_CLEAR`.
static CONSUMER: Mutex<()> = Mutex::new(());
/// Global record sequence number.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Fire the syscall-entry probe for a native syscall.
///
/// Called before dispatch. The check is one relaxed load when no consumer enabled the probe; the
/// control syscalls are excluded so a consumer cannot observe itself.
pub fn probe_syscall(frame: &SyscallFrame) {
    if ENABLED.load(Ordering::Relaxed) & SYSCALL_BIT == 0 {
        return;
    }
    let number = usize::try_from(frame.rax).unwrap_or(usize::MAX);
    if is_control(number) {
        return;
    }
    let cpu = crate::sched::smp::percpu::this_cpu();
    if cpu >= MAX_DTRACE_CPUS {
        return;
    }
    let record = model::Record {
        sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
        timestamp_ns: crate::time::now_ns(),
        arg0: frame.rdi,
        arg1: frame.rsi,
        arg2: frame.rdx,
        arg3: frame.r10,
        syscall: frame.rax as u32,
        tid: crate::sched::current_thread()
            .map(|thread| thread.tid)
            .unwrap_or(0),
        cpu: cpu as u16,
        probe: model::probe::SYSCALL_ENTRY_ID as u16,
    };
    let Some(ring) = RINGS.get(cpu) else {
        return;
    };
    let empty = ring.is_empty();
    if ring.push(record) && empty {
        wake_consumer();
    }
}

/// Signal the consumer's event after records arrive.
fn wake_consumer() {
    let event = WAKE_EVENT.lock().as_ref().map(Arc::clone);
    if let Some(event) = event
        && event.signal()
    {
        crate::event::notify_event(&event);
    }
}

/// Enable a probe. `event` may be `None` when the consumer polls `DTRACE_READ`.
pub fn enable(id: u32, event: Option<Arc<UserEvent>>) -> Result<(), Errno> {
    let probe = model::probe(id).ok_or(Errno::EINVAL)?;
    let bit = 1u64
        .checked_shl(probe.id)
        .filter(|bit| *bit != 0)
        .ok_or(Errno::EINVAL)?;
    if let Some(event) = event {
        *WAKE_EVENT.lock() = Some(event);
    }
    ENABLED.fetch_or(bit, Ordering::AcqRel);
    Ok(())
}

/// Disable a probe.
pub fn disable(id: u32) -> Result<(), Errno> {
    let probe = model::probe(id).ok_or(Errno::EINVAL)?;
    let bit = 1u64
        .checked_shl(probe.id)
        .filter(|bit| *bit != 0)
        .ok_or(Errno::EINVAL)?;
    ENABLED.fetch_and(!bit, Ordering::AcqRel);
    if ENABLED.load(Ordering::Acquire) == 0 {
        *WAKE_EVENT.lock() = None;
    }
    Ok(())
}

/// Drain up to `max` records round-robin across the online CPUs.
pub fn drain(max: usize, records: &mut alloc::vec::Vec<model::Record>) -> usize {
    if max == 0 {
        return 0;
    }
    let _guard = CONSUMER.lock();
    let cpus = crate::sched::smp::online_cpus().min(MAX_DTRACE_CPUS);
    while records.len() < max {
        let mut progress = false;
        for ring in RINGS.iter().take(cpus) {
            if let Some(record) = ring.pop() {
                records.push(record);
                progress = true;
                if records.len() == max {
                    break;
                }
            }
        }
        if !progress {
            break;
        }
    }
    records.len()
}

/// Fill `out` with per-CPU statistics; returns the number of online CPUs represented.
pub fn stats(out: &mut [model::CpuStats]) -> usize {
    let cpus = crate::sched::smp::online_cpus().min(MAX_DTRACE_CPUS);
    let count = out.len().min(cpus);
    for (index, slot) in out.iter_mut().take(count).enumerate() {
        if let Some(ring) = RINGS.get(index) {
            *slot = ring.stats();
        }
    }
    count
}

/// Reset every ring; the caller must have disabled every probe.
pub fn clear() -> Result<(), Errno> {
    if ENABLED.load(Ordering::Acquire) != 0 {
        return Err(Errno::EBUSY);
    }
    let _guard = CONSUMER.lock();
    for ring in &RINGS {
        ring.reset();
    }
    SEQUENCE.store(0, Ordering::Release);
    *WAKE_EVENT.lock() = None;
    Ok(())
}

/// The static probe table.
pub fn probes() -> &'static [model::Probe] {
    model::PROBES
}

/// Fill one user-visible probe record.
pub fn probe_info(probe: &model::Probe) -> model::ProbeInfo {
    let mut info = model::ProbeInfo::default();
    model::probe::fill_info(probe, &mut info);
    info
}
