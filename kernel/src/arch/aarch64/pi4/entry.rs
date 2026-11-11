// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `kernel8.img` entry stub for the Raspberry Pi 4.
//!
//! The firmware enters at EL2 (or EL1 under an emulator) with the DTB in `x0`. The stub parks
//! secondary cores, drops EL2 to EL1 with the physical timer exposed to EL1, sets the early
//! stack, zeroes both BSS areas, and calls the low-linked [`super::early::pi4_early`].

core::arch::global_asm!(
    r#"
    .section .text.pi4._pi_start, "ax"
    .global _pi_start
    .type _pi_start, %function
_pi_start:
    msr daifset, #0xf

    /* Park every core except the boot core (MPIDR affinity 0). */
    mrs x9, mpidr_el1
    and x9, x9, #0xff
    cbnz x9, .Lpark

    /* Accept EL2 (Pi firmware) and EL1 (QEMU's default `virt` entry). */
    mrs x9, currentel
    cmp x9, #0x8
    b.eq .Lel2
    cmp x9, #0x4
    b.eq .Lel1
    b .Lpark

.Lel2:
    mov x19, x0
    /* bias = actual entry address - linked 0x80000 */
    adr x9, _pi_start
    ldr x10, =0x80000
    sub x9, x9, x10

    ldr x10, =__pi_early_stack_top_lma
    add x10, x10, x9
    msr sp_el1, x10

    /* HCR_EL2.RW = 1: EL1 is AArch64. */
    mrs x11, hcr_el2
    mov x10, #1
    lsl x10, x10, #31
    orr x11, x11, x10
    msr hcr_el2, x11

    /* Allow EL1 access to the physical counter and timer. */
    mov x10, #3
    msr cnthctl_el2, x10
    msr cntvoff_el2, xzr

    /* Simulated exception return to EL1h with DAIF masked. */
    mov x10, #0x3c5
    msr spsr_el2, x10
    adr x10, .Lel1
    msr elr_el2, x10
    eret

.Lel1:
    mov x19, x0
    msr spsel, #1
    adr x9, _pi_start
    ldr x10, =0x80000
    sub x9, x9, x10

    ldr x10, =__pi_early_stack_top_lma
    add x10, x10, x9
    mov sp, x10

    /* Zero the kernel's higher-half BSS through its physical address. */
    ldr x11, =__bss_lma_start
    ldr x12, =__bss_lma_end
    add x11, x11, x9
    add x12, x12, x9
1:
    cmp x11, x12
    b.hs 2f
    str xzr, [x11], #8
    b 1b
2:
    mov x0, x19
    mov x1, x9
    bl pi4_early
    b .Lpark

.Lpark:
    wfe
    b .Lpark
    .size _pi_start, . - _pi_start
"#
);
