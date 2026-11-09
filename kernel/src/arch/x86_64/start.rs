// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 kernel entry point and the static boot stack.
//!
//! CONTRACTS C4: the loader enters `_start` with interrupts off, in long mode with paging on, on
//! a loader-provided stack, and `rdi` holding the virtual address of a `raven_bootinfo::BootInfo`.
//! `_start` switches to the kernel's own boot stack and calls `kmain`. It must not touch `rdi`, so
//! the BootInfo pointer arrives in the first SysV argument register.

core::arch::global_asm!(
    r#"
    .section .text._start, "ax"
    .global _start
    .type _start, @function
_start:
    cli
    cld
    lea rsp, [rip + __boot_stack_top]
    xor ebp, ebp
    call kmain
    ud2
    .size _start, . - _start

    .section .bss.__boot_stack, "aw", @nobits
    .balign 16
    .global __boot_stack_bottom
__boot_stack_bottom:
    .skip 65536
    .global __boot_stack_top
__boot_stack_top:
    .size __boot_stack_bottom, __boot_stack_top - __boot_stack_bottom

    /* Nothing calls the exported memory routines yet, and fat LTO would discard them because they
     * are unreachable from the entry point. This table is an opaque reference from assembly, so
     * the LTO pass must keep the definitions; the linker script keeps the table itself. */
    .section .rodata.__raven_mem_symbols, "a"
    .balign 8
    .quad memcpy
    .quad memmove
    .quad memset
    .quad memcmp
    .quad bcmp
"#
);
