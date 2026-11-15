/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/* RavenBSD std-program link script: the generic user layout plus the TLS image and
   the constructor arrays that a std binary carries (uutils K25a). */

ENTRY(_start)

PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
    bss PT_LOAD FLAGS(6);
    tls PT_TLS;
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

    .tdata ALIGN(8) : {
        *(.tdata .tdata.*)
    } :data :tls

    /* The RW LOAD file part must be a page multiple: the end alignment below guarantees it. */
    .data ALIGN(4096) : {
        *(.data .data.*)
        *(.data.rel.ro .data.rel.ro.*)
        /* Rust formatting uses GOT indirections for vtables. They must be loaded. */
        *(.got .got.*)
        . = ALIGN(4096);
    } :data

    .preinit_array ALIGN(8) : {
        PROVIDE_HIDDEN(__preinit_array_start = .);
        KEEP(*(.preinit_array))
        PROVIDE_HIDDEN(__preinit_array_end = .);
    } :data

    .init_array ALIGN(8) : {
        PROVIDE_HIDDEN(__init_array_start = .);
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*)))
        KEEP(*(.init_array))
        PROVIDE_HIDDEN(__init_array_end = .);
    } :data

    .fini_array ALIGN(8) : {
        PROVIDE_HIDDEN(__fini_array_start = .);
        KEEP(*(SORT_BY_INIT_PRIORITY(.fini_array.*)))
        KEEP(*(.fini_array))
        PROVIDE_HIDDEN(__fini_array_end = .);
    } :data

    /* .bss is NOLOAD: it has no file part; spawn maps its zero tail to the page boundary. */
    .bss ALIGN(4096) (NOLOAD) : {
        *(.bss .bss.*)
        *(COMMON)
    } :bss

    .tbss ALIGN(8) (NOLOAD) : {
        *(.tbss .tbss.*)
        *(.tcommon)
    } :tls

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment)
        *(.note*)
        *(.plt*)
    }
}
