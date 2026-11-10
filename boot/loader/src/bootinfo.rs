// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure assembly of the `raven_bootinfo::BootInfo` handed to the kernel.

use crate::layout::HHDM_BASE;
use raven_bootinfo::{
    BOOTINFO_MAGIC, BOOTINFO_VERSION, BootInfo, Framebuffer, PhysRange, arch, flags,
};

/// Architecture id written into `BootInfo`.
#[cfg(target_arch = "x86_64")]
const BOOT_ARCH: u32 = arch::X86_64;
/// Architecture id written into `BootInfo`.
#[cfg(target_arch = "aarch64")]
const BOOT_ARCH: u32 = arch::AARCH64;

/// The values gathered before and after `ExitBootServices`.
#[derive(Clone, Copy, Debug)]
pub struct BootInfoParts {
    /// Physical address of the loaded kernel image.
    pub kernel_phys_base: u64,
    /// Virtual base of the loaded kernel image.
    pub kernel_virt_base: u64,
    /// Page-rounded kernel image size.
    pub kernel_size: u64,
    /// Physical address of the converted `MemoryRegion` array.
    pub memory_map_base: u64,
    /// Number of entries in the converted memory map.
    pub memory_map_len: u64,
    /// Copied initfs image.
    pub initfs: PhysRange,
    /// Copied kernel command line.
    pub cmdline: PhysRange,
    /// Flattened device tree supplied by firmware, or an empty range.
    pub dtb: PhysRange,
    /// Physical address of the ACPI RSDP, or 0.
    pub acpi_rsdp: u64,
    /// GOP framebuffer description, or an empty one.
    pub framebuffer: Framebuffer,
    /// Initial APIC id of the boot CPU.
    pub boot_cpu_id: u64,
    /// TSC value read immediately before the jump.
    pub entry_counter: u64,
    /// TSC frequency in Hz, or 0 when unknown.
    pub counter_freq: u64,
    /// Physical address of the UEFI system table, or 0.
    pub uefi_system_table: u64,
    /// Set the `VERBOSE` flag when the loader log level is `debug`.
    pub verbose: bool,
}

/// Fill the contract structure. The caller must map every physical range before jumping.
pub fn assemble(parts: BootInfoParts) -> BootInfo {
    BootInfo {
        magic: BOOTINFO_MAGIC,
        version: BOOTINFO_VERSION,
        size: core::mem::size_of::<BootInfo>() as u32,
        arch: BOOT_ARCH,
        flags: flags::SERIAL_READY | if parts.verbose { flags::VERBOSE } else { 0 },
        hhdm_offset: HHDM_BASE,
        kernel_phys_base: parts.kernel_phys_base,
        kernel_virt_base: parts.kernel_virt_base,
        kernel_size: parts.kernel_size,
        memory_map_base: parts.memory_map_base,
        memory_map_len: parts.memory_map_len,
        memory_map_entry_size: core::mem::size_of::<raven_bootinfo::MemoryRegion>() as u64,
        initfs: parts.initfs,
        cmdline: parts.cmdline,
        dtb: parts.dtb,
        acpi_rsdp: parts.acpi_rsdp,
        uefi_system_table: parts.uefi_system_table,
        boot_cpu_id: parts.boot_cpu_id,
        entry_counter: parts.entry_counter,
        counter_freq: parts.counter_freq,
        framebuffer: parts.framebuffer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raven_bootinfo::MemoryRegion;

    fn parts() -> BootInfoParts {
        BootInfoParts {
            kernel_phys_base: 0x20_0000,
            kernel_virt_base: 0xFFFF_FFFF_8020_0000,
            kernel_size: 0x12_0000,
            memory_map_base: 0x30_0000,
            memory_map_len: 7,
            initfs: PhysRange {
                base: 0x40_0000,
                len: 0x2000,
            },
            cmdline: PhysRange {
                base: 0x50_0000,
                len: 23,
            },
            dtb: PhysRange { base: 0, len: 0 },
            acpi_rsdp: 0x60_0000,
            framebuffer: Framebuffer {
                base: 0xE000_0000,
                width: 1024,
                height: 768,
                stride_bytes: 4096,
                bits_per_pixel: 32,
                pixel_format: 1,
                reserved: 0,
            },
            boot_cpu_id: 3,
            entry_counter: 0x1234,
            counter_freq: 2_500_000_000,
            uefi_system_table: 0x70_0000,
            verbose: false,
        }
    }

    #[test]
    fn fields_follow_the_contract() {
        let info = assemble(parts());
        assert_eq!(info.magic, BOOTINFO_MAGIC);
        assert_eq!(info.version, BOOTINFO_VERSION);
        assert_eq!(info.size as usize, core::mem::size_of::<BootInfo>());
        assert_eq!(info.arch, BOOT_ARCH);
        assert_eq!(info.hhdm_offset, HHDM_BASE);
        assert_eq!(info.kernel_phys_base, 0x20_0000);
        assert_eq!(info.kernel_virt_base, 0xFFFF_FFFF_8020_0000);
        assert_eq!(info.kernel_size, 0x12_0000);
        assert_eq!(info.memory_map_base, 0x30_0000);
        assert_eq!(info.memory_map_len, 7);
        assert_eq!(
            info.memory_map_entry_size,
            core::mem::size_of::<MemoryRegion>() as u64
        );
        assert_eq!(info.initfs.base, 0x40_0000);
        assert_eq!(info.initfs.len, 0x2000);
        assert_eq!(info.cmdline.base, 0x50_0000);
        assert_eq!(info.cmdline.len, 23);
        assert_eq!(info.dtb.len, 0);
        assert_eq!(info.acpi_rsdp, 0x60_0000);
        assert_eq!(info.uefi_system_table, 0x70_0000);
        assert_eq!(info.boot_cpu_id, 3);
        assert_eq!(info.entry_counter, 0x1234);
        assert_eq!(info.counter_freq, 2_500_000_000);
        assert_eq!(info.framebuffer.width, 1024);
    }

    #[test]
    fn result_validates() {
        assert_eq!(assemble(parts()).validate(), Ok(()));
    }

    #[test]
    fn serial_ready_is_always_set_and_verbose_is_optional() {
        let quiet = assemble(parts());
        assert_eq!(quiet.flags & flags::SERIAL_READY, flags::SERIAL_READY);
        assert_eq!(quiet.flags & flags::VERBOSE, 0);

        let mut verbose = parts();
        verbose.verbose = true;
        let loud = assemble(verbose);
        assert_eq!(loud.flags & flags::VERBOSE, flags::VERBOSE);
    }

    #[test]
    fn empty_ranges_stay_empty() {
        let mut parts = parts();
        parts.initfs = PhysRange { base: 0, len: 0 };
        parts.cmdline = PhysRange { base: 0, len: 0 };
        let info = assemble(parts);
        assert_eq!(info.initfs, PhysRange { base: 0, len: 0 });
        assert_eq!(info.cmdline, PhysRange { base: 0, len: 0 });
    }
}
