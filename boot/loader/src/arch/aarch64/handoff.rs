// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 CPU helpers and the final EL1 handoff.
//!
//! On QEMU `virt` AAVMF enters the loader at EL1 when `virtualization=on` is absent. The ticket
//! still requires an explicit `CurrentEL` check: an EL2 entry is reported and halted rather than
//! attempted with EL1 system-register writes.
//!
//! PSCI `SYSTEM_OFF` uses the `hvc #0` conduit used by QEMU `virt`'s default (non-secure) machine
//! configuration. The `smc #0` fallback is attempted only if the HVC call returns, so a
//! secure-monitor-configured machine also powers off.

use core::arch::asm;

use crate::paging::Roots;

use super::paging::{MAIR, tcr};

/// PSCI `SYSTEM_OFF` function id.
const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;

/// Current exception level (0..=3).
pub fn current_el() -> u64 {
    let value: u64;
    // SAFETY: reading CurrentEL is permitted at every exception level.
    unsafe {
        asm!("mrs {}, currentel", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    (value >> 2) & 3
}

/// True when the loader runs at EL1.
pub fn at_el1() -> bool {
    current_el() == 1
}

/// Physical address width from `ID_AA64MMFR0_EL1.PARange`.
pub fn physical_bits() -> u32 {
    let value: u64;
    // SAFETY: ID_AA64MMFR0_EL1 is readable at EL1.
    unsafe {
        asm!("mrs {}, id_aa64mmfr0_el1", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    match value & 0xf {
        0 => 32,
        1 => 36,
        2 => 40,
        3 => 42,
        4 => 44,
        5 => 48,
        6 => 52,
        _ => 48,
    }
}

/// `MPIDR_EL1 & 0xFF00FFFFFF`: affinity fields of the boot CPU.
pub fn boot_cpu_id() -> u64 {
    let value: u64;
    // SAFETY: MPIDR_EL1 is readable at EL1.
    unsafe {
        asm!("mrs {}, mpidr_el1", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value & 0xFF00_FFFF
}

/// `CNTFRQ_EL0` in Hz.
pub fn counter_freq() -> u64 {
    let value: u64;
    // SAFETY: CNTFRQ_EL0 is readable at EL1.
    unsafe {
        asm!("mrs {}, cntfrq_el0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// `CNTVCT_EL0`: the counter value recorded in `BootInfo.entry_counter`.
pub fn read_counter() -> u64 {
    let value: u64;
    // SAFETY: CNTVCT_EL0 is readable at EL1.
    unsafe {
        asm!("mrs {}, cntvct_el0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// Current stack pointer, used to identity-map the firmware stack.
pub fn stack_pointer() -> u64 {
    let value: u64;
    // SAFETY: reading SP has no side effects.
    unsafe {
        asm!("mov {}, sp", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn clean_kernel_cache(kernel_phys: u64, kernel_size: u64) {
    let start = kernel_phys & !63;
    let end = kernel_phys.saturating_add(kernel_size);
    let mut address = start;
    while address < end {
        // SAFETY: the kernel copy is identity-mapped in TTBR0; `dc cvau` cleans the line
        // containing the address to the point of unification.
        unsafe {
            asm!("dc cvau, {}", in(reg) address, options(nostack));
        }
        address = address.saturating_add(64);
    }
    // SAFETY: barrier and instruction-cache invalidation are architectural maintenance.
    unsafe {
        asm!("dsb ish", "ic iallu", "dsb ish", "isb", options(nostack),);
    }
}

/// Leave UEFI and enter the kernel at EL1.
///
/// # Safety
///
/// `roots` must describe the TTBR0 identity map and the TTBR1 HHDM/kernel map built for this
/// handoff, `stack_top` must point into a mapped writable stack, `bootinfo` must be the HHDM
/// address of a valid `BootInfo`, and `entry` must be the kernel's mapped entry point. The kernel
/// copy at `kernel_phys..kernel_phys + kernel_size` must be identity-mapped. This function does
/// not return.
pub unsafe fn jump(
    entry: u64,
    roots: Roots,
    stack_top: u64,
    bootinfo: u64,
    kernel_phys: u64,
    kernel_size: u64,
) -> ! {
    clean_kernel_cache(kernel_phys, kernel_size);
    let tcr = tcr(physical_bits());
    // Register budget for the trampoline:
    // - `x0` carries the kernel's `BootInfo` argument. It is an `in("x0")` operand and no
    //   instruction here writes it, so it still holds that value at the final branch.
    // - `x16` carries the kernel entry point. It is an `in("x16")` operand and is only read by
    //   the final `br`; `x16`/`x17` are the AAPCS64 intra-procedure-call scratch registers.
    // - `{scratch}` is a compiler-allocated output used for the SCTLR_EL1 read-modify-write.
    //   An output cannot share a register with a live input, so it can never alias `bootinfo`,
    //   `entry` or any other operand, whatever register LLVM picks for it.
    // - MAIR/TCR/TTBR0/TTBR1/stack are read-only inputs consumed by `msr`/`mov sp`. The only
    //   general-purpose register written inside this block is the scratch (`mov sp` writes SP,
    //   not a GPR); `x0` is pinned by its operand constraint and set by the compiler before the
    //   block. The clobber set is therefore explicit and identical in debug and release builds.
    // SAFETY: the caller guarantees the tables and addresses; these are EL1 system-register
    // writes and the final `br` leaves the loader permanently.
    unsafe {
        asm!(
            "msr daifset, #0xf",
            "msr mair_el1, {mair}",
            "msr tcr_el1, {tcr}",
            "msr ttbr0_el1, {ttbr0}",
            "msr ttbr1_el1, {ttbr1}",
            "dsb sy",
            "isb",
            "tlbi vmalle1",
            "dsb sy",
            "isb",
            // UEFI leaves the MMU on. Re-assert M/C/I so a firmware that disabled one of them
            // cannot leave the kernel running uncached; the new TTBR0 identity map is active.
            "mrs {scratch}, sctlr_el1",
            "orr {scratch}, {scratch}, #1",
            "orr {scratch}, {scratch}, #4",
            "orr {scratch}, {scratch}, #0x1000",
            "msr sctlr_el1, {scratch}",
            "isb",
            "mov sp, {stack}",
            "br x16",
            mair = in(reg) MAIR,
            tcr = in(reg) tcr,
            ttbr0 = in(reg) roots.ttbr0,
            ttbr1 = in(reg) roots.ttbr1,
            stack = in(reg) stack_top,
            in("x0") bootinfo,
            in("x16") entry,
            scratch = out(reg) _,
        );
    }
    // The asm only leaves through `br x16`; this is the defensive path for a compiler that
    // thinks the block can fall through.
    halt()
}

/// Power the machine off through PSCI.
pub fn exit_qemu(_code: u8) -> ! {
    // SAFETY: PSCI SYSTEM_OFF is the platform power-off call; if HVC is not the configured
    // conduit the call returns and the SMC fallback is tried.
    unsafe {
        asm!(
            "hvc #0",
            in("x0") PSCI_SYSTEM_OFF,
            options(nomem, nostack),
        );
        asm!(
            "smc #0",
            in("x0") PSCI_SYSTEM_OFF,
            options(nomem, nostack),
        );
    }
    halt()
}

/// Halt the boot CPU.
pub fn halt() -> ! {
    loop {
        // SAFETY: WFI is valid at EL1 and wakes on an interrupt.
        unsafe {
            asm!("wfi", options(nomem, nostack, preserves_flags));
        }
    }
}
