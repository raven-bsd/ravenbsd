/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/*
 * S89 first-stage direct-boot loader.
 *
 * The loader is loaded by QEMU at 1 MiB, below the 2 MiB kernel image base. `_pvh_start` is the
 * 32-bit PVH entry; the Xen PVH note in its own PT_NOTE segment is what QEMU looks for.
 */

ENTRY(_pvh_start)

PHDRS
{
    text PT_LOAD FLAGS(5);
    data PT_LOAD FLAGS(6);
    note PT_NOTE FLAGS(4);
}

SECTIONS
{
    . = 0x100000;
    __loader_start = .;

    .text : {
        *(.text.pvh)
        *(.text .text.*)
    } :text

    .rodata : {
        *(.rodata .rodata.*)
    } :text

    __loader_text_end = .;

    . = ALIGN(4096);
    __loader_data_start = .;
    .data : {
        *(.data .data.*)
    } :data

    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    } :data

    __loader_end = .;

    . = ALIGN(4096);
    .note.Xen : {
        *(.note.Xen)
    } :note

    /DISCARD/ : {
        *(.comment)
        *(.eh_frame .eh_frame.*)
        *(.note.GNU-stack)
        *(.rela*)
        *(.rel.*)
        *(.debug*)
    }
}
