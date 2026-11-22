/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/*
 * AArch64 static userland layout.
 *
 * The PT_TLS program header and the .tdata/.tbss sections are part of the ABI: a static
 * executable with __thread state must tell libc's `__raven_init_main_thread` where its TLS
 * image is. The program headers stay inside the first PT_LOAD so the kernel's AT_PHDR value
 * points at a mapped table and libc can find PT_TLS.
 */

ENTRY(_start)

PHDRS {
    phdr PT_PHDR PHDRS;
    text PT_LOAD FILEHDR PHDRS FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
    bss PT_LOAD FLAGS(6);
    tls PT_TLS FLAGS(4);
}

SECTIONS {
    . = 0x00400000 + SIZEOF_HEADERS;

    /* The R E LOAD file part must be a page multiple: spawn(Strict) rejects a partial tail. */
    .text ALIGN(4096) : {
        *(.text .text.*)
        *(.init .init.*)
        *(.fini .fini.*)
        . = ALIGN(4096);
    } :text

    /* The R LOAD file part must be a page multiple: spawn(Strict) rejects a partial tail. */
    .rodata ALIGN(4096) : {
        *(.rodata .rodata.*)
        . = ALIGN(4096);
    } :rodata

    /* Keep the TLS image separate from .data so PT_TLS has a page-aligned file span. */
    .tdata ALIGN(4096) : {
        *(.tdata .tdata.*)
        . = ALIGN(4096);
    } :data :tls

    /* The RW LOAD file part must be a page multiple: the end alignment below guarantees it. */
    .data ALIGN(4096) : {
        *(.data .data.*)
        *(.data.rel.ro .data.rel.ro.*)
        /* Rust formatting uses GOT indirections for vtables. They must be loaded. */
        *(.got .got.*)
        *(.init_array .init_array.*)
        *(.fini_array .fini_array.*)
        *(.preinit_array .preinit_array.*)
        *(.ctors .ctors.*)
        *(.dtors .dtors.*)
        . = ALIGN(4096);
    } :data

    /* .tbss is NOLOAD but still belongs to the PT_TLS segment. */
    .tbss ALIGN(4096) (NOLOAD) : {
        *(.tbss .tbss.*)
    } :bss :tls

    /* .bss is NOLOAD: it has no file part; spawn maps its zero tail to the page boundary. */
    .bss ALIGN(4096) (NOLOAD) : {
        *(.bss .bss.*)
        *(COMMON)
    } :bss

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment)
        *(.debug*)
        *(.note*)
        *(.plt*)
    }
}
