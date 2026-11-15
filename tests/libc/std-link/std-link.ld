/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * C64e std filesystem probe layout. The userland linker script is for no_std
 * programs; this one keeps the same page-aligned PT_LOAD tails (spawn(Strict)
 * rejects a partial file page) and adds the TLS and init/fini arrays that the
 * Rust standard library needs.
 */

ENTRY(_start)

PHDRS {
    phdr PT_PHDR PHDRS;
    text PT_LOAD FILEHDR PHDRS FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
    tls PT_TLS;
    bss PT_LOAD FLAGS(6);
}

SECTIONS {
    . = 0x00400000;

    .text ALIGN(4096) : {
        *(.text .text.*)
        . = ALIGN(4096);
    } :text

    .rodata ALIGN(4096) : {
        *(.rodata .rodata.*)
        *(.eh_frame_hdr)
        *(.eh_frame)
        . = ALIGN(4096);
    } :rodata

    .tdata ALIGN(4096) : {
        *(.tdata .tdata.*)
        . = ALIGN(4096);
    } :data :tls

    .tbss ALIGN(4096) (NOLOAD) : {
        *(.tbss .tbss.*)
        . = ALIGN(4096);
    } :data :tls

    .init_array ALIGN(4096) : {
        *(.init_array .init_array.*)
        *(.fini_array .fini_array.*)
        . = ALIGN(4096);
    } :data

    .data ALIGN(4096) : {
        *(.data .data.*)
        *(.data.rel.ro .data.rel.ro.*)
        *(.got .got.*)
        *(.got.plt)
        . = ALIGN(4096);
    } :data

    .bss ALIGN(4096) (NOLOAD) : {
        *(.bss .bss.*)
        *(COMMON)
    } :bss

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.plt*)
    }
}
