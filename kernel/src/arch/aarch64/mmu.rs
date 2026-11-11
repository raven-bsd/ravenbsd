// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 MMU, TLB and cache maintenance helpers.
//!
//! The `MAIR_EL1`/`TCR_EL1` values come from `raven-rmm`'s aarch64 backend so the kernel and the
//! loader agree on attr0/attr1 (the kernel extends MAIR with attr2/attr3, which the loader does
//! not use). The TTBR switch follows the ARMv8-A break-before-make sequence: the code that
//! performs the switch runs in the kernel image mapping, which is identical in the old and the
//! new tables, so no instruction fetch can miss mid-switch.

// The cache maintenance API is required by the ticket for later use (code loading in S24, DMA
// buffers in S21d); nothing calls it in S14a yet.
#![allow(dead_code)]

use core::arch::asm;

use raven_rmm::arch::aarch64::{MAIR_EL1, tcr_el1};

/// Physical address width from `ID_AA64MMFR0_EL1.PARange`, capped at the 48-bit descriptor layout.
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
    .min(48)
}

/// Install the kernel's `MAIR_EL1` and `TCR_EL1`.
///
/// Must run before the kernel builds its own tables: table walks after the TTBR switch use these
/// attributes. attr0/attr1 equal the loader's values, so already-mapped memory keeps its
/// attributes.
pub fn init_mair_tcr() {
    let tcr = tcr_el1(physical_bits());
    // SAFETY: MAIR_EL1/TCR_EL1 are EL1 system registers; writing them before the table switch
    // means every walk after the switch sees the new attributes. The barriers order the writes
    // before any subsequent table access.
    unsafe {
        asm!(
            "msr mair_el1, {}",
            "msr tcr_el1, {}",
            "dsb sy",
            "isb",
            in(reg) MAIR_EL1,
            in(reg) tcr,
            options(nostack, preserves_flags),
        );
    }
}

/// Switch TTBR0_EL1 and TTBR1_EL1 to the kernel-owned roots.
///
/// # Safety
///
/// The caller must be executing in a mapping that is identical in the old and new tables (the
/// kernel image), `ttbr1` must be a valid kernel root built for the installed MAIR/TCR, and
/// `ttbr0` must be a zeroed table (the kernel has no user half yet). After this call the loader's
/// table frames may be reclaimed.
pub unsafe fn switch_tables(ttbr0: u64, ttbr1: u64) {
    // SAFETY: the caller guarantees the roots; the barrier sequence is the ARMv8-A
    // break-before-make order also used by the S12 loader on this hardware: make the new tables
    // and register writes visible, switch, then invalidate every cached translation before any
    // further access.
    unsafe {
        asm!(
            "dsb sy",
            "msr ttbr0_el1, {}",
            "msr ttbr1_el1, {}",
            "isb",
            "tlbi vmalle1",
            "dsb sy",
            "isb",
            in(reg) ttbr0,
            in(reg) ttbr1,
            options(nostack, preserves_flags),
        );
    }
}

/// Switch the TTBR0 root and ASID.
///
/// The user half is the only part a process changes; TTBR1 keeps the kernel root. `tlbi
/// aside1is` drops stale translations for the incoming ASID (the user pages carry `nG`), and the
/// architectural `dsb`/`isb` sequence orders the register write before the next translation.
pub fn switch_ttbr0(root: u64, asid: u16) {
    let ttbr0 = (u64::from(asid & 0xff) << 48) | (root & 0x0000_FFFF_FFFF_F000);
    let operand = u64::from(asid & 0xff) << 48;
    // SAFETY: TTBR0_EL1 and TLBI are EL1 operations; the caller passes a root built by the
    // kernel's mapper. The barriers are the ARMv8-A table-switch sequence.
    unsafe {
        asm!(
            "dsb ish",
            "msr ttbr0_el1, {}",
            "isb",
            "tlbi aside1is, {}",
            "dsb ish",
            "isb",
            in(reg) ttbr0,
            in(reg) operand,
            options(nostack, preserves_flags),
        );
    }
}

/// Invalidate one virtual page (`tlbi vaae1is`).
pub fn tlb_flush_page(virtual_address: u64) {
    // SAFETY: `tlbi vaae1is` invalidates the TLB entry for one address at all ASIDs; the ARMv8-A
    // required sequence around it is the dsb/isb pair.
    unsafe {
        asm!(
            "dsb ishst",
            "tlbi vaae1is, {}",
            "dsb ish",
            "isb",
            in(reg) virtual_address >> 12,
            options(nostack, preserves_flags),
        );
    }
}

/// Invalidate every TLB entry for the current EL (`tlbi vmalle1is`).
pub fn tlb_flush_all() {
    // SAFETY: `tlbi vmalle1is` invalidates all EL1 translations at all ASIDs.
    unsafe {
        asm!(
            "dsb ishst",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            options(nostack, preserves_flags),
        );
    }
}

/// The data/unified cache line size in bytes from `CTR_EL0.DminLine`.
fn cache_line_size() -> u64 {
    let value: u64;
    // SAFETY: CTR_EL0 is readable at EL1.
    unsafe {
        asm!("mrs {}, ctr_el0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    4 << ((value >> 16) & 0xf)
}

/// Clean the data cache over `[address, address + len)` (`dc cvac`).
///
/// The range is a virtual range; by-address maintenance does not need cache geometry beyond the
/// line size.
///
/// # Safety
///
/// Every byte of the range must be mapped readable in the current address space.
pub unsafe fn dcache_clean_range(address: u64, len: u64) {
    let line = cache_line_size();
    let start = address & !line.wrapping_sub(1);
    let end = address.saturating_add(len);
    let mut current = start;
    while current < end {
        // SAFETY: the caller guarantees the range is mapped; `dc cvac` cleans the line holding
        // the address to the point of coherency.
        unsafe {
            asm!("dc cvac, {}", in(reg) current, options(nostack));
        }
        current = current.saturating_add(line);
    }
    // SAFETY: the completion barrier is part of the architectural maintenance sequence.
    unsafe {
        asm!("dsb sy", options(nostack, preserves_flags));
    }
}

/// Invalidate the data cache over `[address, address + len)` (`dc ivac`).
///
/// # Safety
///
/// Every byte of the range must be mapped, and the caller must not rely on any dirty line in the
/// range afterwards (invalidate discards dirty data).
pub unsafe fn dcache_invalidate_range(address: u64, len: u64) {
    let line = cache_line_size();
    let start = address & !line.wrapping_sub(1);
    let end = address.saturating_add(len);
    let mut current = start;
    while current < end {
        // SAFETY: the caller guarantees the range is mapped; `dc ivac` invalidates the line.
        unsafe {
            asm!("dc ivac, {}", in(reg) current, options(nostack));
        }
        current = current.saturating_add(line);
    }
    // SAFETY: completion barrier of the maintenance sequence.
    unsafe {
        asm!("dsb sy", options(nostack, preserves_flags));
    }
}

/// Clean and invalidate the data cache over `[address, address + len)` (`dc civac`).
///
/// # Safety
///
/// Every byte of the range must be mapped readable in the current address space.
pub unsafe fn dcache_clean_invalidate_range(address: u64, len: u64) {
    let line = cache_line_size();
    let start = address & !line.wrapping_sub(1);
    let end = address.saturating_add(len);
    let mut current = start;
    while current < end {
        // SAFETY: the caller guarantees the range is mapped; `dc civac` cleans and invalidates
        // the line in one step.
        unsafe {
            asm!("dc civac, {}", in(reg) current, options(nostack));
        }
        current = current.saturating_add(line);
    }
    // SAFETY: completion barrier of the maintenance sequence.
    unsafe {
        asm!("dsb sy", options(nostack, preserves_flags));
    }
}

/// Invalidate the entire instruction cache (`ic iallu`).
///
/// Required after writing executable memory before executing it (code loading, later DMA of
/// instructions).
pub fn icache_invalidate_all() {
    // SAFETY: `ic iallu` invalidates all instruction cache entries to the point of unification;
    // the dsb/isb pair completes the operation before any instruction fetch relies on it.
    unsafe {
        asm!(
            "ic iallu",
            "dsb sy",
            "isb",
            options(nostack, preserves_flags),
        );
    }
}
