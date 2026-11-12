// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interrupt objects on GICv2 (QEMU `virt` and the Pi 4 GIC-400).
//!
//! An `Irq` owns one shared peripheral interrupt (SPI). The object keeps the line masked until
//! the first `IRQ_WAIT` (or `IRQ_ACK`), masks a level line again after every delivery, and
//! releases the SPI claim when the last handle closes. GICv2 has no MSI/MSI-X routing on these
//! boards, so `IRQ_CREATE(MSI|MSIX)` reports `EOPNOTSUPP` instead of inventing a vector ABI.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};

use crate::arch::aarch64::gic;
use crate::event;
use crate::process::{self, handle::Object};
use crate::syscall::SyscallFrame;
use crate::time;

/// `IRQ_CREATE` kinds.
pub const IRQ_LEGACY: u32 = 1;
/// `IRQ_CREATE` kinds.
pub const IRQ_MSI: u32 = 2;
/// `IRQ_CREATE` kinds.
pub const IRQ_MSIX: u32 = 3;

/// Highest SPI id the GICv2 distributor can carry.
const MAX_SPI: u32 = 1020;
/// Registry slots (one per possible interrupt id).
const SLOTS: usize = MAX_SPI as usize;
/// Registry value between claim and publication.
const CLAIMED: u64 = 1;
/// Priority used for every driver line.
const PRIORITY: u8 = 0xa0;

/// One claimed SPI's shared state.
///
/// The GIC handler reaches this through the registry's raw pointer; a waiter holds its own
/// `Arc` so closing the last handle can still run [`Irq::drop`] and wake the waiter.
struct SpiState {
    spi: u32,
    level: bool,
    armed: AtomicBool,
    pending: AtomicU64,
    closed: AtomicBool,
}

impl SpiState {
    fn take_pending(&self) -> Option<usize> {
        let count = self.pending.swap(0, Ordering::AcqRel);
        usize::try_from(count).ok().filter(|count| *count > 0)
    }
}

/// One interrupt source a driver can wait on.
pub struct Irq {
    state: Arc<SpiState>,
}

impl Irq {
    /// Firings accumulated since the last wait/ack.
    pub fn pending(&self) -> u64 {
        self.state.pending.load(Ordering::Acquire)
    }
}

impl Drop for Irq {
    fn drop(&mut self) {
        let spi = self.state.spi;
        self.state.closed.store(true, Ordering::Release);
        let _ = gic::disable(spi);
        let _ = gic::clear_pending(spi);
        let published = Arc::as_ptr(&self.state) as u64;
        {
            // The handler runs on the boot CPU with IRQs masked; masking them here makes the
            // registry clear and the object release one critical section from the handler's
            // view. A handler already in flight has completed before this process context runs.
            let _guard = crate::sync::critical();
            if let Some(slot) = slot(spi) {
                let _ = slot.compare_exchange(published, 0, Ordering::AcqRel, Ordering::Acquire);
            }
        }
    }
}

/// The raw owner registry. Zero is free, [`CLAIMED`] is a creation in progress, any other value
/// is the published `SpiState` pointer.
static OWNERS: [AtomicU64; SLOTS] = [const { AtomicU64::new(0) }; SLOTS];

fn slot(spi: u32) -> Option<&'static AtomicU64> {
    OWNERS.get(usize::try_from(spi).ok()?)
}

/// Handle a claimed SPI from the interrupt vector.
///
/// Returns false when the line is unclaimed; the vector then masks it and counts the event.
pub fn handle_spi(spi: u32) -> bool {
    let Some(slot) = slot(spi) else {
        return false;
    };
    let published = slot.load(Ordering::Acquire);
    if published == 0 || published == CLAIMED {
        return false;
    }
    // SAFETY: `published` was installed by `create` before the source could fire, and `Drop`
    // clears the slot with interrupts masked on the delivery CPU before releasing the object.
    let state = unsafe { &*(published as *const SpiState) };
    if state.level {
        // Storm prevention: a level line is masked before EOI and stays masked until IRQ_ACK.
        let _ = gic::disable(spi);
    }
    state.pending.fetch_add(1, Ordering::AcqRel);
    event::notify_all();
    true
}

/// Report and clear the interrupt-context counters. Process context only.
pub fn flush_spurious() {
    crate::arch::aarch64::irq::flush_spurious();
}

/// `irq_create(root_dev, kind, number, flags) -> Handle`.
pub fn create(frame: &SyscallFrame) -> Result<usize, Errno> {
    flush_spurious();
    let caller = process::current().ok_or(Errno::ESRCH)?;
    super::require_root(&caller, Handle(frame.rdi as u32))?;
    let kind = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let number = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;

    match kind {
        IRQ_LEGACY => {
            // The ABI's fourth argument travels in r10; r8 and r9 are ignored.
            if frame.r10 != 0 {
                return Err(Errno::EINVAL);
            }
            if !gic::is_spi(number) {
                return Err(Errno::EINVAL);
            }
            let route = crate::arch::aarch64::fdt::spi_route(number);
            if route.active_low {
                // GIC-400 SPIs are wired active-high; an active-low tree flag would need a
                // board-level inverter or a different controller, neither of which exists here.
                return Err(Errno::EINVAL);
            }
            let slot = slot(number).ok_or(Errno::EINVAL)?;
            if slot
                .compare_exchange(0, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Err(Errno::EBUSY);
            }
            let configured = gic::set_priority(number, PRIORITY).is_ok()
                && gic::set_target(number, 0).is_ok()
                && gic::configure_spi(number, route.level).is_ok()
                && gic::disable(number).is_ok()
                && gic::clear_pending(number).is_ok();
            if !configured {
                slot.store(0, Ordering::Release);
                return Err(Errno::EINVAL);
            }
            let state = Arc::new(SpiState {
                spi: number,
                level: route.level,
                armed: AtomicBool::new(false),
                pending: AtomicU64::new(0),
                closed: AtomicBool::new(false),
            });
            slot.store(Arc::as_ptr(&state) as u64, Ordering::Release);
            let irq = Arc::new(Irq { state });
            let rights = Rights::READ.union(Rights::DUP).union(Rights::TRANSFER);
            Ok(caller.handles.lock().insert(Object::Irq(irq), rights)?.0 as usize)
        }
        IRQ_MSI | IRQ_MSIX => Err(Errno::EOPNOTSUPP),
        _ => Err(Errno::EINVAL),
    }
}

/// `irq_wait(irq, timeout_ns) -> count`.
pub fn wait(frame: &SyscallFrame) -> Result<usize, Errno> {
    flush_spurious();
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let timeout_ns = frame.rsi;
    let state = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ)? {
            Object::Irq(irq) => Arc::clone(&irq.state),
            _ => return Err(Errno::EBADF),
        }
    };
    // A line stays masked until its first wait (or ack), then each ack re-arms it.
    if !state.armed.swap(true, Ordering::AcqRel) {
        gic::enable(state.spi).map_err(|_| Errno::EINVAL)?;
    }
    let deadline = (timeout_ns != 0).then(|| time::now_ns().saturating_add(timeout_ns));
    // The aarch64 idle context is not re-entered after the first context switch, so a blocked
    // syscall would never be woken by the timer. Poll with interrupts enabled instead: the timer
    // keeps time advancing and the GIC handler publishes the firing. A scheduler fix can restore
    // blocking without changing the object or the ABI.
    // SAFETY: the vector table, GIC and timer are initialised before `/init` runs.
    unsafe {
        crate::sync::enable_interrupts();
    }
    loop {
        if state.closed.load(Ordering::Acquire)
            || caller.handles.lock().get(handle, Rights::READ).is_err()
        {
            return Err(Errno::EIO);
        }
        if let Some(count) = state.take_pending() {
            return Ok(count);
        }
        if let Some(deadline) = deadline
            && time::now_ns() >= deadline
        {
            return Ok(0);
        }
        core::hint::spin_loop();
    }
}

/// `irq_ack(irq) -> 0`.
pub fn ack(frame: &SyscallFrame) -> Result<usize, Errno> {
    flush_spurious();
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let state = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ)? {
            Object::Irq(irq) => Arc::clone(&irq.state),
            _ => return Err(Errno::EBADF),
        }
    };
    if state.closed.load(Ordering::Acquire) {
        return Err(Errno::EIO);
    }
    state.armed.store(true, Ordering::Release);
    // A level source may have set the GIC pending bit again while the line was masked; drop it
    // before re-arming so the next wait starts from a clean edge/level.
    let _ = gic::clear_pending(state.spi);
    gic::enable(state.spi).map_err(|_| Errno::EINVAL)?;
    Ok(0)
}
