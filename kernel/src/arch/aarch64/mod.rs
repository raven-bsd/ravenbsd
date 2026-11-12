// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 architecture support.

pub mod fdt;
pub mod gic;
/// Kernel scheme IPC on aarch64.
///
/// The crate-root `ipc` declaration is x86-64-only; compiling the same sources here gives the
/// aarch64 kernel one IPC module with the registry, namespaces, routing and capture code.
#[path = "../../ipc/mod.rs"]
pub mod ipc;
pub mod irq;
pub mod kmain;
pub mod mmu;
pub mod percpu;
#[cfg(raven_pi4)]
pub mod pi4;
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
