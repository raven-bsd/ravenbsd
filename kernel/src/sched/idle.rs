// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The per-CPU idle loop.
//!
//! Idle runs at the lowest priority and is never enqueued. It halts with interrupts enabled and
//! calls the scheduler only after an interrupt made another thread ready.

/// Run the idle loop forever.
pub fn run() -> ! {
    #[cfg(target_arch = "aarch64")]
    {
        let cpu = crate::sched::smp::percpu::this_cpu();
        if cpu != 0 {
            crate::kprintln!("aarch64-smp: idle cpu={cpu}");
        }
    }
    loop {
        if super::need_resched() {
            let _guard = crate::sync::critical();
            super::schedule_locked();
        }
        #[cfg(target_arch = "x86_64")]
        crate::sched::smp::percpu::note_idle();
        #[cfg(target_arch = "x86_64")]
        // SAFETY: ring-0 `sti; hlt; cli`; the timer gate is installed before the idle loop starts.
        unsafe {
            core::arch::asm!("sti", "hlt", "cli", options(nomem, nostack),);
        }
        #[cfg(target_arch = "aarch64")]
        core::hint::spin_loop();
    }
}
