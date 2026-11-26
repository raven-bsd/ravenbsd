/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/* RavenBSD aarch64 std-program link script for brush (K169).
   Adapted from xtask/brush-x86_64.ld. It keeps the program headers mapped for
   AT_PHDR/PT_TLS discovery, places .tdata/.tbss contiguously for PT_TLS, and
   names the dynamic metadata so lld cannot create an unaligned orphan.
   Every file-backed PT_LOAD ends with an explicit ALIGN(4096); spawn(Strict)
   rejects a partial tail. */

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

    /* R E LOAD; the program headers are part of the file header mapping. */
    .text ALIGN(4096) : {
        *(.text .text.*)
        *(.init .init.*)
        *(.fini .fini.*)
        . = ALIGN(4096);
    } :text

    /* R LOAD. */
    .rodata ALIGN(4096) : {
        *(.rodata .rodata.*)
        . = ALIGN(4096);
    } :rodata

    /* Dynamic read-only metadata; the last section pads the R segment to a page. */
    .dynsym ALIGN(8) : {
        *(.dynsym)
    } :rodata
    .gnu.hash ALIGN(8) : {
        *(.gnu.hash)
    } :rodata
    .hash ALIGN(8) : {
        *(.hash)
    } :rodata
    .dynstr ALIGN(8) : {
        *(.dynstr)
    } :rodata
    .rela.dyn ALIGN(8) : {
        *(.rela.dyn)
        *(.rela.plt)
        *(.rela.*)
        . = ALIGN(4096);
    } :rodata

    /* TLS image: .tdata and .tbss must stay contiguous for local-exec offsets. */
    .tdata ALIGN(4096) : {
        *(.tdata .tdata.*)
    } :data :tls

    .tbss ALIGN(8) (NOLOAD) : {
        *(.tbss .tbss.*)
        *(.tcommon)
    } :data :tls

    /* RW LOAD; the end alignment makes the file tail a page multiple. */
    .data ALIGN(4096) : {
        *(.data .data.*)
        *(.data.rel.ro .data.rel.ro.*)
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

    .dynamic ALIGN(8) : {
        *(.dynamic)
        . = ALIGN(4096);
    } :data

    /* .bss is NOLOAD; spawn maps its zero tail to the page boundary. */
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
        *(.ARM.attributes)
    }
}
