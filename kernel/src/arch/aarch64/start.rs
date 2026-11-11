// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 kernel entry point and the static boot stack.
//!
//! CONTRACTS C4: the loader enters `_start` at EL1 with interrupts masked, MMU on with the
//! loader's tables, and `x0` holding the HHDM virtual address of a `raven_bootinfo::BootInfo`.
//!
//! `_start` masks DAIF, points `VBAR_EL1` at the vector table, switches to the kernel's own
//! 64 KiB boot stack and calls `kmain`. `x0` is preserved because it is the first argument.

core::arch::global_asm!(
    r#"
    .section .text._start, "ax"
    .global _start
    .type _start, %function
_start:
    msr daifset, #0xf
    /* The kernel runs on SP_EL1: lower-EL exceptions then land on the interrupted thread's
     * kernel stack, and SP_EL0 is free for the user stack. */
    msr spsel, #1
    adrp x9, __boot_stack_top
    add x9, x9, :lo12:__boot_stack_top
    mov sp, x9
    adrp x9, __exception_vectors
    add x9, x9, :lo12:__exception_vectors
    msr vbar_el1, x9
    isb
    mov x29, #0
    bl kmain
1:
    wfe
    b 1b
    .size _start, . - _start

    .section .bss.__boot_stack, "aw", @nobits
    .balign 16
    .global __boot_stack_bottom
__boot_stack_bottom:
    .skip 65536
    .global __boot_stack_top
__boot_stack_top:
    .size __boot_stack_bottom, __boot_stack_top - __boot_stack_bottom
"#
);
