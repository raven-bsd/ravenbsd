// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CPU helpers and the final jump to the kernel.

use core::arch::asm;
use core::arch::x86_64::{__cpuid, __cpuid_count, _rdtsc};

use crate::paging::{MAX_PHYSICAL_BITS, MIN_PHYSICAL_BITS};

/// MAXPHYADDR reported by CPUID leaf 0x8000_0008, or 36 when the leaf is unavailable.
pub fn physical_bits() -> u32 {
    let max_extended = __cpuid(0x8000_0000).eax;
    if max_extended >= 0x8000_0008 {
        let bits = __cpuid(0x8000_0008).eax & 0xff;
        if (MIN_PHYSICAL_BITS..=MAX_PHYSICAL_BITS).contains(&bits) {
            return bits;
        }
    }
    36
}

/// Initial APIC id of the boot CPU.
///
/// Use leaf 0xB (extended topology) when the firmware advertises it, otherwise leaf 1 EBX[31:24].
pub fn boot_cpu_id() -> u64 {
    let max_leaf = __cpuid(0).eax;
    if max_leaf >= 0xB {
        let topology = __cpuid_count(0xB, 0);
        if topology.ebx & 0xffff != 0 {
            return u64::from(topology.edx);
        }
    }
    u64::from(__cpuid(1).ebx >> 24)
}

/// TSC frequency from CPUID leaf 0x15 (`ecx * ebx / eax`), or 0 when unknown.
pub fn counter_freq() -> u64 {
    let max_leaf = __cpuid(0).eax;
    if max_leaf < 0x15 {
        return 0;
    }
    let leaf = __cpuid(0x15);
    if leaf.eax == 0 || leaf.ebx == 0 || leaf.ecx == 0 {
        return 0;
    }
    u64::from(leaf.ecx)
        .checked_mul(u64::from(leaf.ebx))
        .map(|value| value / u64::from(leaf.eax))
        .unwrap_or(0)
}

/// Read the time-stamp counter.
pub fn rdtsc() -> u64 {
    // SAFETY: RDTSC is available on every x86-64 CPU.
    unsafe { _rdtsc() }
}

/// Current stack pointer, used to identity-map the firmware stack during the CR3 switch.
pub fn stack_pointer() -> u64 {
    let rsp: u64;
    // SAFETY: reading RSP has no side effects and the compiler may not rely on its value across
    // the asm block because it is declared as an output.
    unsafe {
        asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags));
    }
    rsp
}

/// Leave UEFI: disable interrupts, enable NX, switch to `cr3`, install the kernel stack and jump.
///
/// # Safety
///
/// `cr3` must be a valid page-table root that identity-maps the currently executing code and the
/// old stack, `stack_top` must point into a mapped writable stack, `bootinfo` must be the HHDM
/// address of a valid `BootInfo`, and `entry` must be the kernel's mapped entry point. This
/// function does not return.
pub unsafe fn jump(entry: u64, cr3: u64, stack_top: u64, bootinfo: u64) -> ! {
    // SAFETY: the caller guarantees the page tables and addresses; `rdmsr`/`wrmsr` are ring-0
    // instructions and the MSR number is the architectural IA32_EFER.
    unsafe {
        asm!(
            "cli",
            "mov ecx, 0xC0000080",
            "rdmsr",
            "bts eax, 11",
            "wrmsr",
            "mov cr3, rsi",
            "mov rsp, r8",
            "mov rdi, r9",
            "jmp r10",
            in("rsi") cr3,
            in("r8") stack_top,
            in("r9") bootinfo,
            in("r10") entry,
            options(noreturn),
        )
    }
}
