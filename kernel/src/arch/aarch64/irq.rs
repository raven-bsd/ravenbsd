// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IRQ dispatch from the exception vectors.
//!
//! The vector entry saves a full trap frame and calls [`dispatch`] with interrupts masked for the
//! duration (the CPU masks IRQ on exception entry). The handler acknowledges the GIC, runs the
//! device handler, signals end-of-interrupt and then gives the scheduler a chance to switch
//! before the frame is restored. Nothing here allocates or blocks.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::arch::aarch64::{gic, timer};
use crate::sched;

static IRQ_COUNT: AtomicU64 = AtomicU64::new(0);
/// PPI number of the timer, zero until [`set_timer`].
static TIMER_IRQ: AtomicU32 = AtomicU32::new(0);

/// Record the timer interrupt line (called after GIC/timer init).
pub fn set_timer(irq: u32) {
    TIMER_IRQ.store(irq, Ordering::Release);
}

/// Number of interrupts dispatched since boot.
pub fn count() -> u64 {
    IRQ_COUNT.load(Ordering::Acquire)
}

/// Handle one IRQ exception. Called from `exception_dispatch` for the IRQ vector slots.
///
/// `from_user` is true for the lower-EL slots: only those may be preempted on return (the x86-64
/// `timer_return(from_user)` policy). Returns after the interrupt and any resulting context
/// switch; the vector then restores the interrupted frame.
pub fn dispatch(from_user: bool) {
    let raw = gic::acknowledge();
    let id = gic::interrupt_id(raw);
    if gic::is_spurious(id) {
        return;
    }
    IRQ_COUNT.fetch_add(1, Ordering::Relaxed);

    if id == TIMER_IRQ.load(Ordering::Acquire) {
        timer::handle();
    } else {
        // No other driver registers a handler yet (S14b). Disable the line so an unexpected
        // interrupt cannot storm, and report it once.
        let _ = gic::disable(id);
        crate::kprintln!("irq: unexpected interrupt {}", id);
    }

    gic::eoi(raw);
    if sched::initialized() {
        sched::tick();
        sched::irq_return(from_user);
    }
}
