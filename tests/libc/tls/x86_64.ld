/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/*
 * C73 on-target C link layout. `spawn(LoadMode::Strict)` rejects a PT_LOAD whose file part
 * ends in a partial 4096-byte page, so every file-backed section is rounded to a page. The
 * layout mirrors userland/linker/x86_64.ld but keeps the C runtime sections and TLS.
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
    /*
     * Keep the ELF/program headers inside the first LOAD. `__raven_init_main_thread` reads
     * AT_PHDR/AT_PHNUM to find the executable's PT_TLS; a layout that starts the first LOAD at
     * file offset 0x1000 leaves the headers unmapped and the kernel writes AT_PHDR=0.
     */
    . = 0x00400000 + SIZEOF_HEADERS;

    .text ALIGN(4096) : {
        *(.text .text.*)
        *(.init .init.*)
        *(.fini .fini.*)
        *(.plt .plt.*)
        . = ALIGN(4096);
    } :text

    .rodata ALIGN(4096) : {
        *(.rodata .rodata.*)
        *(.eh_frame .eh_frame.*)
        *(.gcc_except_table .gcc_except_table.*)
        *(.note .note.*)
        . = ALIGN(4096);
    } :rodata

    .tdata ALIGN(4096) : {
        *(.tdata .tdata.*)
        . = ALIGN(4096);
    } :data :tls

    .data ALIGN(4096) : {
        *(.data .data.*)
        *(.data.rel.ro .data.rel.ro.*)
        *(.got .got.*)
        . = ALIGN(8);
        __preinit_array_start = .;
        *(.preinit_array .preinit_array.*)
        __preinit_array_end = .;
        __init_array_start = .;
        *(.init_array .init_array.*)
        __init_array_end = .;
        *(.fini_array .fini_array.*)
        *(.ctors .ctors.*)
        *(.dtors .dtors.*)
        . = ALIGN(4096);
    } :data

    .tbss ALIGN(4096) (NOLOAD) : {
        *(.tbss .tbss.*)
        /* S188: keep the TLS mem tail page-rounded for the Strict service spawn. */
        . = ALIGN(4096);
    } :bss :tls

    .bss ALIGN(4096) (NOLOAD) : {
        *(.bss .bss.*)
        *(COMMON)
    } :bss

    /DISCARD/ : {
        /* S188: orphan unwind metadata after .rodata breaks the Strict file-tail check. */
        *(.eh_frame*)
        *(.note*)
        *(.comment)
        *(.debug*)
    }
}
