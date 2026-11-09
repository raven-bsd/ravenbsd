/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

/* The kernel image lives in the top 2 GiB; the low half of the top 2 GiB is kept free so the
 * image starts at KERNEL_IMAGE_BASE + 2 MiB. Physical addresses are the virtual addresses minus
 * KERNEL_OFFSET, keeping p_paddr small enough for any boot loader. */
KERNEL_OFFSET = 0xFFFFFFFF80000000;
KERNEL_BASE = 0xFFFFFFFF80200000;

PHDRS {
    text PT_LOAD FLAGS(5);   /* R E */
    rodata PT_LOAD FLAGS(4); /* R   */
    data PT_LOAD FLAGS(6);   /* RW  */
}

SECTIONS {
    . = KERNEL_BASE;

    .text ALIGN(4096) : AT(ADDR(.text) - KERNEL_OFFSET) {
        __kernel_start = .;
        __text_start = .;
        KEEP(*(.text._start))
        *(.text .text.*)
        __text_end = .;
    } :text

    .rodata ALIGN(4096) : AT(ADDR(.rodata) - KERNEL_OFFSET) {
        __rodata_start = .;
        KEEP(*(.rodata.__raven_mem_symbols))
        *(.rodata .rodata.*)
        __rodata_end = .;
    } :rodata

    .data ALIGN(4096) : AT(ADDR(.data) - KERNEL_OFFSET) {
        __data_start = .;
        *(.data .data.*)
        __data_end = .;
    } :data

    .bss ALIGN(4096) (NOLOAD) : AT(ADDR(.bss) - KERNEL_OFFSET) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    } :data

    __kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment)
        *(.note.GNU-stack)
    }
}
