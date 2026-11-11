/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

ENTRY(_start)

PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}

SECTIONS {
    . = 0x00400000;

    .text ALIGN(4096) : {
        KEEP(*(.text._start))
        *(.text .text.*)
        . = ALIGN(4096);
    } :text

    .rodata ALIGN(4096) : {
        *(.rodata .rodata.*)
        . = ALIGN(4096);
    } :rodata

    .data ALIGN(4096) : {
        *(.data .data.*)
        *(.got .got.*)
        *(.got.plt)
        . = ALIGN(4096);
    } :data

    .bss ALIGN(4096) (NOLOAD) : {
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4096);
    } :data

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment)
        *(.note*)
    }
}
