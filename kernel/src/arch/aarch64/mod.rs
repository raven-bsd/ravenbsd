// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 architecture support.

pub mod fdt;
pub mod gic;
pub mod irq;
pub mod kmain;
pub mod mmu;
pub mod percpu;
pub mod selftest;
pub mod serial;
pub mod start;
pub mod syscall;
pub mod timer;
pub mod vectors;

/// Halts the current CPU forever.
///
/// Interrupts stay masked: the early-boot kernel never enables them, so `wfi` only wakes on an
/// SError or debug event, after which the loop halts again.
pub fn halt() -> ! {
    loop {
        // SAFETY: WFI is valid at EL1 and touches no memory or stack.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
        }
    }
}
