// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Low-linked Pi 4 early boot: build the first page tables, enable the MMU and enter the
//! higher-half Rust code.
//!
//! Everything reachable before `msr sctlr_el1` runs from `.text.pi4` at the image load address.
//! The page-table builder returns `Option`/`bool` and uses wrapping arithmetic only; the memory
//! accessors below use inline assembly, so no outlined `core` helper can pull the pre-MMU path
//! into the higher-half image.

#![allow(dead_code)] // Called from `_pi_start` in the entry assembly.

use core::arch::asm;

use crate::arch::HHDM_BASE;

use super::paging::imp::{DEVICE, EXECUTABLE, MAIR, PAGE_SIZE, PageTables, PhysMem, WRITABLE, tcr};

/// Kernel virtual base, matching the linker script.
const KERNEL_BASE: u64 = 0xFFFF_FFFF_8020_0000;
/// Number of 4 KiB frames reserved for the early tables.
const POOL_FRAMES: usize = 64;
/// Pi 4 peripheral window (PL011, GIC-400, GPIO, mailbox).
const PI_MMIO_BASE: u64 = 0xFC00_0000;
/// Pi 4 peripheral window length.
const PI_MMIO_LEN: u64 = 0x0390_0000;
/// QEMU `virt` low device window (GIC, PL011, RTC).
const QEMU_MMIO_BASE: u64 = 0x0800_0000;
/// QEMU `virt` device window length.
const QEMU_MMIO_LEN: u64 = 0x0200_0000;
/// The `n`-th byte of the initfs magic, as an immediate.
///
/// A `const [u8; 8]` would be materialised in `.rodata`, whose high-half VMA is not mapped before
/// the MMU is enabled; the pre-MMU initfs probe must not touch it.
#[inline(always)]
const fn initfs_magic(index: usize) -> u8 {
    match index {
        0 => b'R',
        1 => b'A',
        2 => b'V',
        3 => b'E',
        4 => b'N',
        5 => b'F',
        6 => b'S',
        7 => b'1',
        _ => 0,
    }
}

/// Frames for the early translation tables.
#[used]
#[unsafe(link_section = ".bss.pi4_pool")]
static mut PAGE_POOL: [u64; 512 * POOL_FRAMES] = [0; 512 * POOL_FRAMES];

/// Stack used between the firmware entry and the switch to the kernel's boot stack.
#[used]
#[allow(dead_code)] // The assembly names the section's LMA symbol, not the Rust item.
#[unsafe(link_section = ".bss.pi4_early_stack")]
static mut EARLY_STACK: [u8; 64 * 1024] = [0; 64 * 1024];

unsafe extern "C" {
    static __pi_image_start: u8;
    static __pi_text_start: u8;
    static __pi_text_end: u8;
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __data_start: u8;
    static __data_end: u8;
    static __bss_start: u8;
    static __bss_end: u8;
    static __boot_stack_top: u8;
    static __exception_vectors: u8;
    static __kernel_lma: u8;
    static __image_mem_end: u8;
    static __pi_pool_lma: u8;
    static __pi_pool_end_lma: u8;
    static __pi_early_stack_bottom_lma: u8;
    static __pi_early_stack_top_lma: u8;
}

struct PiPhysMem {
    next: u64,
    end: u64,
}

impl PhysMem for PiPhysMem {
    #[inline(always)]
    fn allocate_frame(&mut self) -> Option<u64> {
        let next = self.next.wrapping_add(PAGE_SIZE);
        if next > self.end {
            return None;
        }
        let frame = self.next;
        self.next = next;
        Some(frame)
    }

    #[inline(always)]
    fn zero_frame(&mut self, physical: u64) {
        let mut index = 0usize;
        while index < 512 {
            let address = physical.wrapping_add((index as u64) << 3);
            // SAFETY: `physical` is a frame from the static pool; before the MMU physical and
            // virtual addresses are identical.
            unsafe {
                asm!("str xzr, [{address}]", address = in(reg) address, options(nostack));
            }
            index = index.wrapping_add(1);
        }
    }

    #[inline(always)]
    fn read_u64(&self, physical: u64) -> u64 {
        let value: u64;
        // SAFETY: table frames are in the static pool and identity-accessible before the MMU.
        unsafe {
            asm!(
                "ldr {value}, [{address}]",
                value = out(reg) value,
                address = in(reg) physical,
                options(nostack),
            );
        }
        value
    }

    #[inline(always)]
    fn write_u64(&mut self, physical: u64, value: u64) {
        // SAFETY: table frames are in the static pool and identity-accessible before the MMU.
        unsafe {
            asm!(
                "str {value}, [{address}]",
                value = in(reg) value,
                address = in(reg) physical,
                options(nostack),
            );
        }
    }
}

/// Low-linked continuation after the EL2 drop or direct EL1 entry.
///
/// # Safety
///
/// `dtb_phys` is the firmware's DTB physical pointer and `bias` is the actual load address of the
/// flat image minus the linked `0x80000` (QEMU `virt` loads `-kernel` at `0x4008_0000`, the Pi
/// firmware at `0x80000`).
#[unsafe(link_section = ".text.pi4")]
#[unsafe(no_mangle)]
pub extern "C" fn pi4_early(dtb_phys: u64, bias: u64) -> ! {
    // Read each linker symbol separately: a large tuple move makes the compiler call `memcpy`,
    // which lives in the higher-half image and cannot run before the MMU.
    macro_rules! sym {
        ($name:ident) => {{
            // SAFETY: the linker symbols describe this image; taking their addresses has no
            // memory side effects.
            unsafe { symbol(&$name) }
        }};
    }
    let pi_text_start = sym!(__pi_text_start);
    let pi_text_end = sym!(__pi_text_end);
    let text_start = sym!(__text_start).wrapping_sub(bias);
    let text_end = sym!(__text_end).wrapping_sub(bias);
    let rodata_start = sym!(__rodata_start).wrapping_sub(bias);
    let rodata_end = sym!(__rodata_end).wrapping_sub(bias);
    let data_start = sym!(__data_start).wrapping_sub(bias);
    let data_end = sym!(__data_end).wrapping_sub(bias);
    let bss_start = sym!(__bss_start).wrapping_sub(bias);
    let bss_end = sym!(__bss_end).wrapping_sub(bias);
    let boot_stack_top = sym!(__boot_stack_top).wrapping_sub(bias);
    let exception_vectors = sym!(__exception_vectors).wrapping_sub(bias);
    let kernel_lma_symbol = sym!(__kernel_lma);
    let image_end_low = sym!(__image_mem_end);
    let pool_lma = sym!(__pi_pool_lma);
    let pool_end_lma = sym!(__pi_pool_end_lma);
    let early_stack_bottom_lma = sym!(__pi_early_stack_bottom_lma);
    let early_stack_top_lma = sym!(__pi_early_stack_top_lma);
    let bits = physical_bits();
    let mut tables = PageTables::new_uninit(
        PiPhysMem {
            next: pool_lma,
            end: pool_end_lma,
        },
        bits,
    );
    if !tables.init() {
        low_halt();
    }

    let kernel_lma = kernel_lma_symbol;
    let kernel_offset = KERNEL_BASE.wrapping_sub(kernel_lma);

    // Low identity map for the stub and its pre-MMU stack. The early stack must stay addressable
    // after `msr sctlr_el1` returns through it, so its physical range is mapped too.
    if !tables.map_identity(
        pi_text_start,
        pi_text_end.wrapping_sub(pi_text_start),
        EXECUTABLE,
    ) {
        low_halt();
    }
    let stack_bottom = early_stack_bottom_lma;
    let stack_top = early_stack_top_lma;
    if stack_top <= stack_bottom
        || !tables.map_identity(stack_bottom, stack_top.wrapping_sub(stack_bottom), WRITABLE)
    {
        low_halt();
    }

    // HHDM of usable RAM. The kernel touches RAM below its own image while it builds its frame
    // allocator, so the whole window must be mapped before the MMU is enabled, not just the
    // image. QEMU `virt` RAM starts at 1 GiB; Pi 4 low RAM starts at zero.
    let (ram_base, ram_len) = if dtb_phys >= 0x4000_0000 {
        (0x4000_0000u64, 0x4000_0000u64)
    } else {
        (0u64, 0x8000_0000u64)
    };
    if !tables.map_hhdm_region(HHDM_BASE, ram_base, ram_len, WRITABLE) {
        low_halt();
    }

    // Higher-half kernel segments.
    if !map_segment(&mut tables, text_start, text_end, EXECUTABLE, kernel_offset) {
        low_halt();
    }
    if !map_segment(&mut tables, rodata_start, rodata_end, 0, kernel_offset) {
        low_halt();
    }
    if !map_segment(&mut tables, data_start, data_end, WRITABLE, kernel_offset) {
        low_halt();
    }
    if !map_segment(&mut tables, bss_start, bss_end, WRITABLE, kernel_offset) {
        low_halt();
    }

    // The image lies inside the RAM window mapped above; its HHDM alias needs no extra map.
    let image_end = image_end_low;

    // HHDM of the DTB, read from the firmware header before the MMU is on.
    let dtb_len = match read_be_u32_phys(dtb_phys.wrapping_add(4)) {
        Some(length) => length,
        None => 0,
    };
    if dtb_len >= 40 {
        let start = dtb_phys & !(PAGE_SIZE - 1);
        let end = align_up(dtb_phys.wrapping_add(u64::from(dtb_len)));
        if end > start {
            let _ = tables.map_hhdm_region(HHDM_BASE, start, end.wrapping_sub(start), 0);
        }
    }

    // HHDM of an initfs appended after the image, if one is present.
    if let Some(length) = appended_initfs_len(image_end) {
        let end = align_up(image_end.wrapping_add(length));
        if end > image_end {
            let _ =
                tables.map_hhdm_region(HHDM_BASE, image_end, end.wrapping_sub(image_end), WRITABLE);
        }
    }

    // Device windows used by the console and the GIC on both boards.
    let _ = tables.map_hhdm_region(HHDM_BASE, PI_MMIO_BASE, PI_MMIO_LEN, WRITABLE | DEVICE);
    let _ = tables.map_hhdm_region(HHDM_BASE, QEMU_MMIO_BASE, QEMU_MMIO_LEN, WRITABLE | DEVICE);

    enable_mmu(tables.ttbr0(), tables.ttbr1(), tcr(bits));
    // The high half is live now: point the vectors at the kernel's table and switch to the
    // kernel's own boot stack before running the high-half builder.
    // SAFETY: the higher-half image and BSS are mapped read/write and read/execute by the tables
    // just installed.
    unsafe {
        asm!(
            "msr vbar_el1, {vectors}",
            "isb",
            "mov sp, {stack}",
            vectors = in(reg) exception_vectors,
            stack = in(reg) boot_stack_top,
            options(nostack),
        );
    }
    super::pi4_main(dtb_phys, bias)
}

#[inline(always)]
fn map_segment(
    tables: &mut PageTables<PiPhysMem>,
    start: u64,
    end: u64,
    flags: u32,
    kernel_offset: u64,
) -> bool {
    if end <= start {
        return true;
    }
    let physical = start.wrapping_sub(kernel_offset);
    tables.map_kernel_range(start, physical, end.wrapping_sub(start), flags)
}

#[inline(always)]
#[unsafe(link_section = ".text.pi4")]
fn enable_mmu(ttbr0: u64, ttbr1: u64, tcr: u64) {
    // SAFETY: the two roots and the attributes were built from the static pool; the caller is
    // executing in the low identity mapping that the new root contains.
    unsafe {
        asm!(
            "msr ttbr0_el1, {ttbr0}",
            "msr ttbr1_el1, {ttbr1}",
            "msr mair_el1, {mair}",
            "msr tcr_el1, {tcr}",
            "dsb sy",
            "isb",
            "mrs {sctlr}, sctlr_el1",
            "orr {sctlr}, {sctlr}, #(1 << 0)",
            "orr {sctlr}, {sctlr}, #(1 << 2)",
            "orr {sctlr}, {sctlr}, #(1 << 12)",
            "msr sctlr_el1, {sctlr}",
            "isb",
            ttbr0 = in(reg) ttbr0,
            ttbr1 = in(reg) ttbr1,
            mair = in(reg) MAIR,
            tcr = in(reg) tcr,
            sctlr = lateout(reg) _,
            options(nostack),
        );
    }
}

// No `match`: release builds lower a dense match into a `.rodata` jump table, and `.rodata` is
// not mapped at its virtual address before the MMU is enabled.
#[inline(always)]
#[allow(clippy::arithmetic_side_effects)] // `parange <= 15`, so every operation is bounded.
#[unsafe(link_section = ".text.pi4")]
fn physical_bits() -> u32 {
    let value: u64;
    // SAFETY: ID_AA64MMFR0_EL1 is readable at EL1.
    unsafe {
        asm!("mrs {}, id_aa64mmfr0_el1", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    let parange = (value & 0xf) as u32;
    if parange <= 2 {
        32 + parange * 4
    } else if parange <= 4 {
        36 + parange * 2
    } else {
        48
    }
}

#[unsafe(link_section = ".text.pi4")]
fn appended_initfs_len(physical: u64) -> Option<u64> {
    let mut index = 0usize;
    while index < 8 {
        let byte: u64;
        let address = physical.wrapping_add(index as u64);
        // SAFETY: before the MMU, the physical address is readable; the magic range is read only.
        unsafe {
            asm!(
                "ldrb {byte:w}, [{address}]",
                byte = out(reg) byte,
                address = in(reg) address,
                options(nostack),
            );
        }
        if (byte as u8) != initfs_magic(index) {
            return None;
        }
        index = index.wrapping_add(1);
    }
    let mut length = 0u64;
    index = 0;
    while index < 8 {
        let byte: u64;
        let address = physical.wrapping_add(40).wrapping_add(index as u64);
        // SAFETY: same physical range as the magic; the initfs header is 64 bytes.
        unsafe {
            asm!(
                "ldrb {byte:w}, [{address}]",
                byte = out(reg) byte,
                address = in(reg) address,
                options(nostack),
            );
        }
        length |= (byte & 0xff) << (index << 3);
        index = index.wrapping_add(1);
    }
    if length >= 64 && length <= 64 * 1024 * 1024 {
        Some(length)
    } else {
        None
    }
}

#[unsafe(link_section = ".text.pi4")]
fn read_be_u32_phys(physical: u64) -> Option<u32> {
    if physical == 0 {
        return None;
    }
    let value: u32;
    // SAFETY: the DTB header is readable through the physical pointer before the MMU.
    unsafe {
        asm!(
            "ldr {value:w}, [{address}]",
            "rev {value:w}, {value:w}",
            value = out(reg) value,
            address = in(reg) physical,
            options(nostack),
        );
    }
    Some(value)
}

#[inline(always)]
fn align_up(value: u64) -> u64 {
    value.wrapping_add(PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

#[inline(always)]
fn symbol(address: *const impl Sized) -> u64 {
    address as *const () as u64
}

#[unsafe(link_section = ".text.pi4")]
fn low_halt() -> ! {
    loop {
        // SAFETY: WFE is valid at EL1 and touches no memory.
        unsafe {
            asm!("wfe", options(nomem, nostack, preserves_flags));
        }
    }
}
