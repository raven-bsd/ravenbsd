// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `BootInfo` contract between a boot path (UEFI loader, Pi 4 entry stub) and the kernel.
//!
//! LEAD-OWNED. See `ravenbsd-prjmgmt/architecture/02-boot-and-platforms.md` section 2.
//! The structure is `#[repr(C)]`, versioned, and its layout is const-asserted below. This crate does no
//! pointer dereferencing: the kernel (which owns `unsafe`) turns the physical addresses into slices.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

/// `"RAVENBSD"` as a little-endian `u64`.
pub const BOOTINFO_MAGIC: u64 = u64::from_le_bytes(*b"RAVENBSD");
/// Contract version. Bumped when the layout changes.
pub const BOOTINFO_VERSION: u32 = 1;

/// Architecture identifiers.
pub mod arch {
    /// x86-64.
    pub const X86_64: u32 = 1;
    /// AArch64.
    pub const AARCH64: u32 = 2;
    /// RISC-V 64.
    pub const RISCV64: u32 = 3;
}

/// Flag bits in [`BootInfo::flags`].
pub mod flags {
    /// The boot path wants verbose kernel logging.
    pub const VERBOSE: u32 = 1 << 0;
    /// The loader found a serial console and initialised it (kernel may reuse it).
    pub const SERIAL_READY: u32 = 1 << 1;
}

/// A physical address range. `len == 0` means "absent".
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PhysRange {
    /// Physical start address.
    pub base: u64,
    /// Length in bytes.
    pub len: u64,
}

impl PhysRange {
    /// True if the range is absent.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// One past the last byte, or `None` on overflow.
    pub const fn end(&self) -> Option<u64> {
        self.base.checked_add(self.len)
    }
}

/// Kind of a physical memory region.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemoryKind {
    /// Free RAM the kernel may use immediately.
    Usable = 0,
    /// Loader page tables and the `BootInfo` itself; reclaimable after the kernel copied what it needs.
    LoaderReclaimable = 1,
    /// The kernel image and boot modules (initfs).
    KernelAndModules = 2,
    /// ACPI tables, reclaimable after parsing.
    AcpiReclaimable = 3,
    /// ACPI non-volatile storage; never touch.
    AcpiNvs = 4,
    /// Reserved by firmware or hardware.
    Reserved = 5,
    /// Memory-mapped I/O.
    Mmio = 6,
    /// Defective memory.
    BadMemory = 7,
}

impl MemoryKind {
    /// Convert a raw value.
    pub const fn from_u32(v: u32) -> Option<MemoryKind> {
        Some(match v {
            0 => MemoryKind::Usable,
            1 => MemoryKind::LoaderReclaimable,
            2 => MemoryKind::KernelAndModules,
            3 => MemoryKind::AcpiReclaimable,
            4 => MemoryKind::AcpiNvs,
            5 => MemoryKind::Reserved,
            6 => MemoryKind::Mmio,
            7 => MemoryKind::BadMemory,
            _ => return None,
        })
    }
}

/// One entry of the physical memory map. Entries are sorted by `base`, page aligned, non-overlapping.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MemoryRegion {
    /// Physical start address (4 KiB aligned).
    pub base: u64,
    /// Length in bytes (multiple of 4 KiB, non-zero).
    pub len: u64,
    /// A [`MemoryKind`] value.
    pub kind: u32,
    /// Must be zero.
    pub reserved: u32,
}

impl MemoryRegion {
    /// One past the last byte, or `None` on overflow.
    pub const fn end(&self) -> Option<u64> {
        self.base.checked_add(self.len)
    }

    /// Typed kind, `None` if the raw value is unknown.
    pub const fn kind(&self) -> Option<MemoryKind> {
        MemoryKind::from_u32(self.kind)
    }
}

/// UEFI GOP-style framebuffer. `width == 0` means none.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Framebuffer {
    /// Physical base address.
    pub base: u64,
    /// Width in pixels (0 = no framebuffer).
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bytes per scanline.
    pub stride_bytes: u32,
    /// Bits per pixel.
    pub bits_per_pixel: u32,
    /// 0 = RGB (R lowest byte), 1 = BGR, 2 = bitmask/other.
    pub pixel_format: u32,
    /// Must be zero.
    pub reserved: u32,
}

/// The handoff structure. Passed to the kernel as a virtual (HHDM) pointer.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BootInfo {
    /// [`BOOTINFO_MAGIC`].
    pub magic: u64,
    /// [`BOOTINFO_VERSION`].
    pub version: u32,
    /// `size_of::<BootInfo>()` as the boot path built it.
    pub size: u32,
    /// An [`arch`] value.
    pub arch: u32,
    /// [`flags`] bits.
    pub flags: u32,
    /// Virtual address at which physical address 0 is mapped.
    pub hhdm_offset: u64,
    /// Physical address of the kernel image.
    pub kernel_phys_base: u64,
    /// Virtual address of the kernel image.
    pub kernel_virt_base: u64,
    /// Size of the kernel image in bytes.
    pub kernel_size: u64,
    /// Physical address of the first [`MemoryRegion`].
    pub memory_map_base: u64,
    /// Number of memory map entries.
    pub memory_map_len: u64,
    /// Size of one entry in bytes (`size_of::<MemoryRegion>()`; allows growth).
    pub memory_map_entry_size: u64,
    /// The initfs image (boot archive).
    pub initfs: PhysRange,
    /// Kernel command line (UTF-8, not NUL terminated).
    pub cmdline: PhysRange,
    /// Flattened device tree blob, if any.
    pub dtb: PhysRange,
    /// Physical address of the ACPI RSDP, or 0.
    pub acpi_rsdp: u64,
    /// Physical address of the UEFI system table, or 0.
    pub uefi_system_table: u64,
    /// APIC id / MPIDR / hartid of the boot CPU.
    pub boot_cpu_id: u64,
    /// Raw cycle counter read as late as possible by the boot path (boot timing T1).
    pub entry_counter: u64,
    /// Frequency of `entry_counter` in Hz (0 if unknown).
    pub counter_freq: u64,
    /// Framebuffer, if any.
    pub framebuffer: Framebuffer,
}

/// Why a `BootInfo` was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BootInfoError {
    /// Wrong magic.
    BadMagic,
    /// Unsupported version.
    BadVersion,
    /// `size` smaller than this crate's `BootInfo`.
    TooSmall,
    /// Memory map entry size smaller than [`MemoryRegion`].
    BadEntrySize,
    /// Memory map missing or empty.
    NoMemoryMap,
    /// `hhdm_offset` is zero or not page aligned.
    BadHhdm,
    /// Unknown architecture value.
    BadArch,
}

impl BootInfo {
    /// Validate the fixed part of the structure (the kernel validates the memory map contents with
    /// [`validate_memory_map`] after mapping it).
    pub fn validate(&self) -> Result<(), BootInfoError> {
        if self.magic != BOOTINFO_MAGIC {
            return Err(BootInfoError::BadMagic);
        }
        if self.version != BOOTINFO_VERSION {
            return Err(BootInfoError::BadVersion);
        }
        if (self.size as usize) < core::mem::size_of::<BootInfo>() {
            return Err(BootInfoError::TooSmall);
        }
        if !(arch::X86_64..=arch::RISCV64).contains(&self.arch) {
            return Err(BootInfoError::BadArch);
        }
        if self.hhdm_offset == 0 || !self.hhdm_offset.is_multiple_of(4096) {
            return Err(BootInfoError::BadHhdm);
        }
        if self.memory_map_base == 0 || self.memory_map_len == 0 {
            return Err(BootInfoError::NoMemoryMap);
        }
        if (self.memory_map_entry_size as usize) < core::mem::size_of::<MemoryRegion>() {
            return Err(BootInfoError::BadEntrySize);
        }
        Ok(())
    }

    /// An all-zero value (not valid; a starting point for boot paths and tests).
    pub const fn zeroed() -> BootInfo {
        BootInfo {
            magic: 0,
            version: 0,
            size: 0,
            arch: 0,
            flags: 0,
            hhdm_offset: 0,
            kernel_phys_base: 0,
            kernel_virt_base: 0,
            kernel_size: 0,
            memory_map_base: 0,
            memory_map_len: 0,
            memory_map_entry_size: 0,
            initfs: PhysRange { base: 0, len: 0 },
            cmdline: PhysRange { base: 0, len: 0 },
            dtb: PhysRange { base: 0, len: 0 },
            acpi_rsdp: 0,
            uefi_system_table: 0,
            boot_cpu_id: 0,
            entry_counter: 0,
            counter_freq: 0,
            framebuffer: Framebuffer {
                base: 0,
                width: 0,
                height: 0,
                stride_bytes: 0,
                bits_per_pixel: 0,
                pixel_format: 0,
                reserved: 0,
            },
        }
    }
}

/// Why a memory map was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemoryMapError {
    /// An entry is not page aligned, has zero length or overflows.
    BadEntry(usize),
    /// An entry has an unknown kind or non-zero reserved field.
    BadKind(usize),
    /// Entries are not sorted by base or overlap.
    Unsorted(usize),
}

/// Check the documented invariants: page aligned, non-empty, no overflow, known kinds, sorted,
/// non-overlapping.
pub fn validate_memory_map(map: &[MemoryRegion]) -> Result<(), MemoryMapError> {
    let mut prev_end = 0u64;
    for (i, r) in map.iter().enumerate() {
        if !r.base.is_multiple_of(4096) || !r.len.is_multiple_of(4096) || r.len == 0 {
            return Err(MemoryMapError::BadEntry(i));
        }
        let Some(end) = r.end() else {
            return Err(MemoryMapError::BadEntry(i));
        };
        if r.kind().is_none() || r.reserved != 0 {
            return Err(MemoryMapError::BadKind(i));
        }
        if i > 0 && r.base < prev_end {
            return Err(MemoryMapError::Unsorted(i));
        }
        prev_end = end;
    }
    Ok(())
}

/// Sum of the lengths of all `Usable` regions (saturating).
pub fn total_usable(map: &[MemoryRegion]) -> u64 {
    map.iter()
        .filter(|r| r.kind() == Some(MemoryKind::Usable))
        .fold(0u64, |acc, r| acc.saturating_add(r.len))
}

const _: () = assert!(core::mem::size_of::<PhysRange>() == 16);
const _: () = assert!(core::mem::size_of::<MemoryRegion>() == 24);
const _: () = assert!(core::mem::size_of::<Framebuffer>() == 32);
const _: () = assert!(core::mem::size_of::<BootInfo>() == 200);
const _: () = assert!(core::mem::align_of::<BootInfo>() == 8);
const _: () = assert!(core::mem::offset_of!(BootInfo, hhdm_offset) == 24);
const _: () = assert!(core::mem::offset_of!(BootInfo, memory_map_base) == 56);
const _: () = assert!(core::mem::offset_of!(BootInfo, initfs) == 80);
const _: () = assert!(core::mem::offset_of!(BootInfo, acpi_rsdp) == 128);
const _: () = assert!(core::mem::offset_of!(BootInfo, entry_counter) == 152);
const _: () = assert!(core::mem::offset_of!(BootInfo, framebuffer) == 168);

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> BootInfo {
        let mut b = BootInfo::zeroed();
        b.magic = BOOTINFO_MAGIC;
        b.version = BOOTINFO_VERSION;
        b.size = core::mem::size_of::<BootInfo>() as u32;
        b.arch = arch::X86_64;
        b.hhdm_offset = 0xFFFF_8000_0000_0000;
        b.memory_map_base = 0x1000;
        b.memory_map_len = 2;
        b.memory_map_entry_size = 24;
        b
    }

    #[test]
    fn magic_spells_ravenbsd() {
        assert_eq!(&BOOTINFO_MAGIC.to_le_bytes(), b"RAVENBSD");
    }

    #[test]
    fn valid_passes() {
        assert_eq!(good().validate(), Ok(()));
    }

    #[test]
    fn rejections() {
        let mut b = good();
        b.magic = 1;
        assert_eq!(b.validate(), Err(BootInfoError::BadMagic));
        let mut b = good();
        b.version = 99;
        assert_eq!(b.validate(), Err(BootInfoError::BadVersion));
        let mut b = good();
        b.size = 8;
        assert_eq!(b.validate(), Err(BootInfoError::TooSmall));
        let mut b = good();
        b.arch = 9;
        assert_eq!(b.validate(), Err(BootInfoError::BadArch));
        let mut b = good();
        b.hhdm_offset = 0;
        assert_eq!(b.validate(), Err(BootInfoError::BadHhdm));
        let mut b = good();
        b.memory_map_len = 0;
        assert_eq!(b.validate(), Err(BootInfoError::NoMemoryMap));
        let mut b = good();
        b.memory_map_entry_size = 8;
        assert_eq!(b.validate(), Err(BootInfoError::BadEntrySize));
    }

    fn region(base: u64, len: u64, kind: MemoryKind) -> MemoryRegion {
        MemoryRegion {
            base,
            len,
            kind: kind as u32,
            reserved: 0,
        }
    }

    #[test]
    fn memory_map_checks() {
        let ok = [
            region(0, 0x1000, MemoryKind::Reserved),
            region(0x10_0000, 0x40_0000, MemoryKind::Usable),
            region(0x50_0000, 0x1000, MemoryKind::KernelAndModules),
        ];
        assert_eq!(validate_memory_map(&ok), Ok(()));
        assert_eq!(total_usable(&ok), 0x40_0000);

        let unaligned = [region(1, 0x1000, MemoryKind::Usable)];
        assert_eq!(
            validate_memory_map(&unaligned),
            Err(MemoryMapError::BadEntry(0))
        );

        let empty = [region(0, 0, MemoryKind::Usable)];
        assert_eq!(
            validate_memory_map(&empty),
            Err(MemoryMapError::BadEntry(0))
        );

        let overlap = [
            region(0, 0x2000, MemoryKind::Usable),
            region(0x1000, 0x1000, MemoryKind::Usable),
        ];
        assert_eq!(
            validate_memory_map(&overlap),
            Err(MemoryMapError::Unsorted(1))
        );

        let overflow = [region(0xFFFF_FFFF_FFFF_F000, 0x2000, MemoryKind::Usable)];
        assert_eq!(
            validate_memory_map(&overflow),
            Err(MemoryMapError::BadEntry(0))
        );

        let mut bad_kind = region(0, 0x1000, MemoryKind::Usable);
        bad_kind.kind = 99;
        assert_eq!(
            validate_memory_map(&[bad_kind]),
            Err(MemoryMapError::BadKind(0))
        );
    }
}
