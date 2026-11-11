// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 architecture support.

pub mod acpi;
pub mod ap_start;
pub mod apic;
pub mod exception;
pub mod gdt;
pub mod idt;
pub mod ioapic;
pub mod ipi;
pub mod msi;
pub mod pic;
pub mod serial;
mod start;
pub mod syscall;
pub mod timer;
pub mod trapframe;
pub mod tsc;
#[allow(dead_code)] // `enter` is replaced by the S09 scheduler trampoline; kept for the S08 ABI.
pub mod usermode;
pub mod vector;

/// Enable maskable interrupts.
///
/// Must only be called after the timer and spurious IDT gates are installed and the legacy PIC is
/// masked. This is the last point at which the BSP has the local APIC, timer and scheduler ready
/// with interrupts still off, so it is also where the application processors are brought online.
pub fn enable_interrupts() {
    crate::sched::smp::platform_init();
    // SAFETY: `sti` is a ring-0 instruction that only sets IF; all interrupt gates are installed
    // by the caller before this point.
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
    }
    crate::sched::smp::after_enable_interrupts();
}

/// Halts the current CPU forever.
///
/// Interrupts are disabled first so that a stray interrupt cannot leave the CPU spinning in the
/// halt loop; `hlt` still wakes on NMI and machine checks, after which the loop halts again.
pub fn halt() -> ! {
    loop {
        // SAFETY: `cli` and `hlt` are privileged instructions, valid in ring 0, and touch no
        // memory. They do not change the stack and `cli`/`hlt` leave the flags alone.
        unsafe {
            core::arch::asm!("cli", "hlt", options(nomem, nostack, preserves_flags),);
        }
    }
}
