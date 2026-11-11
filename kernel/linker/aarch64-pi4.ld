/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

OUTPUT_FORMAT(elf64-littleaarch64)
ENTRY(_pi_start)

/* The Pi firmware loads the flat image at 0x80000 and enters the first instruction.
 *
 * The image is linked with a low entry section at 0x80000 and the kernel at its usual
 * higher-half VMA. The higher-half sections' LMAs follow the low stub in the flat file; the
 * pre-MMU stack and page-table pool are ordinary BSS statics whose physical LMAs are published as
 * absolute linker symbols for the low entry code.
 */
KERNEL_BASE = 0xFFFFFFFF80200000;
PI_LOAD = 0x80000;

PHDRS {
    boot PT_LOAD FLAGS(5);   /* R E */
    text PT_LOAD FLAGS(5);   /* R E */
    rodata PT_LOAD FLAGS(4); /* R   */
    data PT_LOAD FLAGS(6);   /* RW  */
    bss PT_LOAD FLAGS(6);    /* RW, no file contents */
}

SECTIONS {
    . = PI_LOAD;
    __pi_image_start = .;

    .text.boot ALIGN(16) : {
        __pi_text_start = .;
        KEEP(*(.text.pi4._pi_start))
        KEEP(*(.text.pi4))
        KEEP(*(.text.boot .text.boot.*))
        . = ALIGN(4096);
        __pi_text_end = .;
    } :boot
    __pi_text_end_abs = LOADADDR(.text.boot) + SIZEOF(.text.boot);
    __kernel_offset_linked = KERNEL_BASE - __pi_text_end_abs;

    . = KERNEL_BASE;
    .text ALIGN(4096) : AT(__pi_text_end) {
        __kernel_start = .;
        __text_start = .;
        KEEP(*(.text._start))
        *(.text .text.*)
        __text_end = .;
    } :text

    .rodata ALIGN(4096) : AT(ADDR(.rodata) - __kernel_offset_linked) {
        __rodata_start = .;
        KEEP(*(.rodata.__raven_mem_symbols))
        *(.rodata .rodata.*)
        __rodata_end = .;
    } :rodata

    .data ALIGN(4096) : AT(ADDR(.data) - __kernel_offset_linked) {
        __data_start = .;
        *(.data .data.*)
        __data_end = .;
    } :data

    /* BSS follows the loaded data in physical memory, page aligned. Its VMA keeps the same
     * VMA-to-LMA offset as the rest of the higher-half image. */
    .bss ALIGN(4096) (NOLOAD) : AT(ADDR(.bss) - __kernel_offset_linked) {
        __bss_start = .;
        . = ALIGN(4096);
        KEEP(*(.bss.__boot_stack))
        . = ALIGN(16);
        __pi_pool_start_vma = .;
        KEEP(*(.bss.pi4_pool))
        __pi_pool_end_vma = .;
        . = ALIGN(16);
        __pi_early_stack_bottom_vma = .;
        KEEP(*(.bss.pi4_early_stack))
        . = ALIGN(16);
        __pi_early_stack_top_vma = .;
        . = ALIGN(4096);
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(16);
        __bss_end = .;
    } :bss

    __bss_lma_start = ABSOLUTE(ADDR(.bss) - __kernel_offset_linked);
    __bss_lma_end = ABSOLUTE(__bss_lma_start + SIZEOF(.bss));
    __pi_pool_lma = ABSOLUTE(__bss_lma_start + (__pi_pool_start_vma - __bss_start));
    __pi_pool_end_lma = ABSOLUTE(__bss_lma_start + (__pi_pool_end_vma - __bss_start));
    __pi_early_stack_bottom_lma = ABSOLUTE(__bss_lma_start + (__pi_early_stack_bottom_vma - __bss_start));
    __pi_early_stack_top_lma = ABSOLUTE(__bss_lma_start + (__pi_early_stack_top_vma - __bss_start));

    __kernel_end = .;
    __image_mem_end = __bss_lma_end;
    __kernel_lma = __pi_text_end_abs;
    __kernel_size = __kernel_end - __kernel_start;
    __kernel_offset = __kernel_offset_linked;

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment)
        *(.note.GNU-stack)
    }
}
