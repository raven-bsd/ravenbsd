/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * Local copy of the C64e std layout for the target `raven` binary. The userland
 * linker script is for no_std programs; this one keeps the same page-aligned
 * PT_LOAD tails (spawn(Strict) rejects a partial file page) and adds the TLS
 * and init/fini arrays that the Rust standard library needs. The only local
 * change is placing `.debug_gdb_scripts` (an orphan Rust section) inside the
 * aligned rodata block; otherwise it ends the rodata PT_LOAD mid-page.
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
        *(.debug_gdb_scripts)
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
