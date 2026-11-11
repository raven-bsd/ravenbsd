/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

ENTRY(_start)

PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
    bss PT_LOAD FLAGS(6);
}

SECTIONS {
    . = 0x00400000;

    /* The R E LOAD file part must be a page multiple: spawn(Strict) rejects a partial tail. */
    .text ALIGN(4096) : {
        *(.text .text.*)
        . = ALIGN(4096);
    } :text

    /* The R LOAD file part must be a page multiple: spawn(Strict) rejects a partial tail. */
    .rodata ALIGN(4096) : {
        *(.rodata .rodata.*)
        . = ALIGN(4096);
    } :rodata

    /* The RW LOAD file part must be a page multiple: the end alignment below guarantees it. */
    .data ALIGN(4096) : {
        *(.data .data.*)
        *(.data.rel.ro .data.rel.ro.*)
        /* Rust formatting uses GOT indirections for vtables. They must be loaded. */
        *(.got .got.*)
        . = ALIGN(4096);
    } :data

    /* .bss is NOLOAD: it has no file part; spawn maps its zero tail to the page boundary. */
    .bss ALIGN(4096) (NOLOAD) : {
        *(.bss .bss.*)
        *(COMMON)
    } :bss

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment)
        *(.note*)
        *(.plt*)
    }
}
