/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/* RavenBSD std-program link script: the generic user layout plus the TLS image and
   the constructor arrays that a std binary carries (uutils K25a).

   The kernel's spawn(Strict) rejects two PT_LOAD segments that share a page and a
   file part that does not end on a page boundary. Every file-backed segment below
   therefore ends with an explicit ALIGN(4096); the dynamic metadata is named so it
   lands in the segment it belongs to instead of becoming an unaligned orphan. */

ENTRY(_start)

PHDRS {
    phdr PT_PHDR PHDRS;
    text PT_LOAD FILEHDR PHDRS FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
    bss PT_LOAD FLAGS(6);
    tls PT_TLS;
}

SECTIONS {
    /* The first LOAD maps the ELF and program headers, so AT_PHDR names a mapped table and
       libc can find PT_TLS. Keep the image base at 0x00400000; .text starts after the headers
       and is page-aligned, so the file part still ends on a page boundary. */
    . = 0x00400000 + SIZEOF_HEADERS;

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

    /* The TLS image starts on its own page, so the RW LOAD segment never shares the
       R segment's last page. */
    .tdata ALIGN(4096) : {
        *(.tdata .tdata.*)
    } :data :tls

    /* .tbss must be contiguous with .tdata in the PT_TLS image: the compiler's local-exec
       offsets are relative to that block, not to .bss. */
    .tbss ALIGN(8) (NOLOAD) : {
        *(.tbss .tbss.*)
        *(.tcommon)
    } :data :tls

    /* The RW LOAD file part must be a page multiple. */
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

    /* The last file-backed RW section pads the segment to a page. */
    .dynamic ALIGN(8) : {
        *(.dynamic)
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
