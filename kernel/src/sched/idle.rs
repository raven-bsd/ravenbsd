// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The per-CPU idle loop.
//!
//! Idle runs at the lowest priority and is never enqueued. It halts with interrupts enabled and
//! calls the scheduler only after an interrupt made another thread ready.

/// Run the idle loop forever.
pub fn run() -> ! {
    loop {
        if super::need_resched() {
            let _guard = crate::sync::critical();
            super::schedule_locked();
        }
        // SAFETY: ring-0 `sti; hlt; cli`; the timer gate is installed before the idle loop starts.
        unsafe {
            core::arch::asm!("sti", "hlt", "cli", options(nomem, nostack),);
        }
    }
}
