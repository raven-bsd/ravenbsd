// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raspberry Pi 4 boot path.
//!
//! The firmware loads `kernel8.img` at `0x80000` and enters `_pi_start` at EL2 (or EL1 on some
//! emulators) with `x0` = the DTB physical address. The low entry stub builds minimal page tables
//! and drops to EL1; [`pi4_main`] then builds the `BootInfo` from the FDT and enters the common
//! `kmain`.

#![allow(dead_code)] // The entry assembly references `pi4_early`; Rust's dead-code pass cannot see it.

pub mod early;
pub mod entry;
pub mod paging;

use core::cell::UnsafeCell;

use raven_bootinfo::{BootInfo, PhysRange};

use crate::arch::HHDM_BASE;
use crate::arch::aarch64::fdt;
use crate::kprintln;

use super::serial;

/// Kernel virtual base written by the Pi linker script.
const KERNEL_BASE: u64 = 0xFFFF_FFFF_8020_0000;
/// Maximum DTB accepted from firmware.
const MAX_DTB: u64 = 4 * 1024 * 1024;
/// Maximum appended initfs accepted.
const MAX_INITFS: u64 = 64 * 1024 * 1024;

struct BootInfoCell(UnsafeCell<BootInfo>);

// SAFETY: the Pi path writes the cell once during single-CPU boot before the scheduler or any
// other context exists; `kmain` only reads the HHDM copy afterwards.
unsafe impl Sync for BootInfoCell {}

static BOOT_INFO: BootInfoCell = BootInfoCell(UnsafeCell::new(BootInfo::zeroed()));

struct MapCell(UnsafeCell<[raven_bootinfo::MemoryRegion; fdt::MAX_REGIONS]>);

// SAFETY: filled once before `kmain` and never mutated afterwards.
unsafe impl Sync for MapCell {}

static MEMORY_MAP: MapCell = MapCell(UnsafeCell::new(
    [raven_bootinfo::MemoryRegion {
        base: 0,
        len: 0,
        kind: 0,
        reserved: 0,
    }; fdt::MAX_REGIONS],
));

struct CmdlineCell(UnsafeCell<[u8; fdt::CMDLINE_MAX]>);

// SAFETY: filled once before `kmain` and only read through the copied BootInfo afterwards.
unsafe impl Sync for CmdlineCell {}

static CMDLINE: CmdlineCell = CmdlineCell(UnsafeCell::new([0; fdt::CMDLINE_MAX]));

unsafe extern "C" {
    static __pi_image_start: u8;
    static __kernel_lma: u8;
    static __image_mem_end: u8;
    static __text_start: u8;
    static __kernel_end: u8;
}

/// High-half continuation after the early MMU set-up.
///
/// # Safety
///
/// Called once by the Pi entry stub with the TTBR1 tables active, the kernel image mapped at its
/// VMA, and `bias` equal to the actual load address minus the linked `0x80000`.
pub extern "C" fn pi4_main(dtb_phys: u64, bias: u64) -> ! {
    let dtb_len = match read_be_u32_phys(dtb_phys, 4) {
        Some(length) if u64::from(length) >= 40 && u64::from(length) <= MAX_DTB => {
            u64::from(length)
        }
        _ => {
            kprintln!("kernel: error: invalid DTB at 0x{dtb_phys:x}");
            crate::arch::halt();
        }
    };
    let Some(fdt) = fdt::dtb(dtb_phys, dtb_len) else {
        kprintln!("kernel: error: cannot parse the DTB");
        crate::arch::halt();
    };
    let platform = fdt::platform_from_fdt(&fdt);
    serial::set_uart(platform.uart, platform.uart_clock_hz);

    // SAFETY: the linker symbols describe this image; taking their addresses has no side effects.
    let (image_start_low, image_end_low, kernel_lma_symbol, text_start, kernel_end) = unsafe {
        (
            symbol(&__pi_image_start),
            symbol(&__image_mem_end),
            symbol(&__kernel_lma),
            symbol(&__text_start),
            symbol(&__kernel_end),
        )
    };
    let image_start = image_start_low.wrapping_add(bias);
    let image_end = image_end_low.wrapping_add(bias);
    let kernel_lma = kernel_lma_symbol.wrapping_add(bias);
    let kernel_offset = KERNEL_BASE.wrapping_sub(kernel_lma);
    let (bootinfo_phys, map_phys, cmdline_phys) = match physical_addresses(kernel_offset) {
        Some(addresses) => addresses,
        None => {
            kprintln!("kernel: error: bad kernel load offset");
            crate::arch::halt();
        }
    };

    let initfs = appended_initfs(image_end);
    let params = fdt::Params {
        hhdm_offset: HHDM_BASE,
        kernel_phys_base: kernel_lma,
        kernel_virt_base: KERNEL_BASE,
        kernel_size: kernel_end.wrapping_sub(text_start),
        image_phys_base: image_start,
        image_size: image_end.wrapping_sub(image_start),
        dtb_phys,
        dtb_len,
        initfs,
        boot_cpu_id: mpidr(),
        counter_freq: cntfrq(),
        entry_counter: cntpct(),
        memory_map_phys: map_phys,
        cmdline_phys,
    };

    // SAFETY: the boot path is single-threaded and the scheduler has not started; both buffers
    // are exclusive to this function for the duration of the call.
    let (map, cmdline) = unsafe { (&mut *MEMORY_MAP.0.get(), &mut *CMDLINE.0.get()) };
    let info = match fdt::build_into(&fdt, &params, map, cmdline) {
        Ok(info) => info,
        Err(error) => {
            kprintln!("kernel: error: bootinfo: {error}");
            crate::arch::halt();
        }
    };
    // SAFETY: written once, before `kmain` reads the HHDM copy.
    unsafe {
        BOOT_INFO.0.get().write(info);
    }
    let boot = HHDM_BASE.wrapping_add(bootinfo_phys) as *const BootInfo;
    crate::arch::aarch64::kmain::kmain(boot)
}

fn physical_addresses(kernel_offset: u64) -> Option<(u64, u64, u64)> {
    let bootinfo = symbol(&BOOT_INFO).checked_sub(kernel_offset)?;
    let map = symbol(&MEMORY_MAP).checked_sub(kernel_offset)?;
    let cmdline = symbol(&CMDLINE).checked_sub(kernel_offset)?;
    Some((bootinfo, map, cmdline))
}

fn symbol(address: *const impl Sized) -> u64 {
    address as *const () as u64
}

fn appended_initfs(physical: u64) -> PhysRange {
    let Some(magic) = read_phys(physical, 8) else {
        return PhysRange { base: 0, len: 0 };
    };
    if magic != *b"RAVENFS1" {
        return PhysRange { base: 0, len: 0 };
    }
    let Some(bytes) = read_phys(physical.saturating_add(40), 8) else {
        return PhysRange { base: 0, len: 0 };
    };
    let mut word = [0u8; 8];
    word.copy_from_slice(&bytes);
    let length = u64::from_le_bytes(word);
    if length < 64 || length > MAX_INITFS {
        return PhysRange { base: 0, len: 0 };
    }
    PhysRange {
        base: physical,
        len: length,
    }
}

fn read_phys(physical: u64, length: usize) -> Option<[u8; 8]> {
    if length > 8 {
        return None;
    }
    let virtual_address = HHDM_BASE.checked_add(physical)?;
    let mut bytes = [0u8; 8];
    // SAFETY: the HHDM maps the image range and the appended initfs; `length` bytes are read.
    unsafe {
        core::ptr::copy_nonoverlapping(virtual_address as *const u8, bytes.as_mut_ptr(), length);
    }
    Some(bytes)
}

fn read_be_u32_phys(physical: u64, offset: u64) -> Option<u32> {
    let virtual_address = HHDM_BASE.checked_add(physical)?.checked_add(offset)?;
    // SAFETY: the DTB range is mapped through the HHDM; the header is at least 8 bytes.
    let value = unsafe { (virtual_address as *const u32).read_volatile() };
    Some(u32::from_be(value))
}

fn mpidr() -> u64 {
    let value: u64;
    // SAFETY: MPIDR_EL1 is readable at EL1.
    unsafe {
        core::arch::asm!("mrs {}, mpidr_el1", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn cntfrq() -> u64 {
    let value: u64;
    // SAFETY: CNTFRQ_EL0 is readable at EL1.
    unsafe {
        core::arch::asm!("mrs {}, cntfrq_el0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn cntpct() -> u64 {
    let value: u64;
    // SAFETY: CNTPCT_EL0 is readable at EL1.
    unsafe {
        core::arch::asm!("mrs {}, cntpct_el0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}
