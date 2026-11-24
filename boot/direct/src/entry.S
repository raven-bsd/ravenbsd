/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/*
 * S89 direct-boot entry.
 *
 * QEMU's PVH option ROM enters this ELF in 32-bit protected mode with %ebx pointing at the
 * `hvm_start_info`. The stub builds a 4-level identity map for the first 4 GiB (2 MiB leaves),
 * enables PAE/LME/paging/NX, switches to a flat 64-bit code segment and calls the Rust entry
 * with the start-info physical address in %edi (the SysV first argument).
 */

.section .text.pvh, "ax"
.globl _pvh_start
.type _pvh_start, @function
.code32
_pvh_start:
    cli
    cld
    movl %ebx, %esi

    lgdt __gdt_desc
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movl $__loader_stack_top, %esp
    xorl %ebp, %ebp

    /* Zero PML4 and PDPT (8 KiB). */
    movl $__boot_pml4, %edi
    xorl %eax, %eax
    movl $2048, %ecx
    rep stosl

    /* PML4[0] -> PDPT. */
    movl $__boot_pdpt, %eax
    orl $3, %eax
    movl %eax, __boot_pml4
    movl $0, __boot_pml4 + 4

    /* PDPT[0..3] -> the four 1 GiB page directories. */
    movl $__boot_pdpt, %edi
    movl $__boot_pd, %eax
    movl $4, %ecx
1:
    movl %eax, %edx
    orl $3, %edx
    movl %edx, (%edi)
    movl $0, 4(%edi)
    addl $4096, %eax
    addl $8, %edi
    loop 1b

    /* 2048 * 2 MiB leaves cover physical 0..4 GiB. */
    movl $__boot_pd, %edi
    xorl %ebx, %ebx
    movl $2048, %ecx
2:
    movl %ebx, %eax
    shll $21, %eax
    orl $0x83, %eax
    movl %eax, (%edi)
    movl $0, 4(%edi)
    addl $8, %edi
    incl %ebx
    loop 2b

    /* PAE, CR3, EFER.LME|NXE, CR0.PG|PE. */
    movl %cr4, %eax
    orl $0x20, %eax
    movl %eax, %cr4
    movl $__boot_pml4, %eax
    movl %eax, %cr3
    movl $0xC0000080, %ecx
    rdmsr
    orl $0x900, %eax
    wrmsr
    movl %cr0, %eax
    orl $0x80000001, %eax
    movl %eax, %cr0
    ljmpl $0x08, $pvh_start64
.size _pvh_start, . - _pvh_start

.code64
pvh_start64:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movl %esi, %edi
    movq $__loader_stack_top, %rsp
    xorq %rbp, %rbp
    call rust_entry
    ud2

.section .rodata
.balign 8
__gdt:
    .quad 0
    .quad 0x00AF9A000000FFFF
    .quad 0x00CF92000000FFFF
__gdt_end:
.balign 2
__gdt_desc:
    .word __gdt_end - __gdt - 1
    .long __gdt

.section .bss
.balign 4096
__boot_pml4:
    .skip 4096
__boot_pdpt:
    .skip 4096
__boot_pd:
    .skip 16384
.balign 16
__loader_stack_bottom:
    .skip 131072
__loader_stack_top:

.section .note.Xen, "a", @note
.balign 4
.long 4
.long 4
.long 18
.asciz "Xen"
.balign 4
.long _pvh_start
