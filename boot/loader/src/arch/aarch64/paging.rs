// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 4 KiB-granule, 48-bit-VA page tables for the loader handoff.
//!
//! # Architectural facts applied here
//!
//! * 4 KiB granule and 48-bit virtual addresses; `TCR_EL1.T0SZ = T1SZ = 16`.
//! * `TTBR0` identity-maps the loader image, its stack, the page-table pool and the kernel image
//!   copy (the last for `dc cvau` before execution). `TTBR1` maps the HHDM with 2 MiB blocks and
//!   the kernel segments with 4 KiB pages.
//! * `MAIR_EL1` attribute 0 is device-nGnRnE (`0x00`), attribute 1 is normal write-back (`0xFF`).
//! * Descriptors set `AF`, inner shareability and the correct `AP`/`PXN`/`UXN` bits. Nothing is
//!   mapped writable and executable.
//!
//! The encoder is generic over [`PhysMem`] so the host tests exercise the exact bit patterns with a
//! `Vec<u8>`-backed fake.

use crate::paging_core::{
    MAX_PHYSICAL_BITS, MIN_PHYSICAL_BITS, PAGE_SIZE, PAGE_SIZE_2M, PageError, PhysMem,
};

/// Output-address mask for the 48-bit VA/PA format used here.
pub const ADDRESS_MASK: u64 = 0x0000_FFFF_FFFF_F000;
/// Descriptor is valid (present).
pub const VALID: u64 = 1 << 0;
/// Table descriptor or L3 page descriptor (bit 1 set); clear on an L1/L2 block.
pub const TABLE_OR_PAGE: u64 = 1 << 1;
/// Access flag.
pub const AF: u64 = 1 << 10;
/// Inner-shareable attribute.
pub const SH_INNER: u64 = 0b11 << 8;
/// Attribute index 0: device-nGnRnE.
pub const ATTR_DEVICE: u64 = 0b000 << 2;
/// Attribute index 1: normal write-back.
pub const ATTR_NORMAL: u64 = 0b001 << 2;
/// EL1 read/write.
pub const AP_RW: u64 = 0b00 << 6;
/// EL1 read-only.
pub const AP_RO: u64 = 0b10 << 6;
/// Privileged execute-never.
pub const PXN: u64 = 1 << 53;
/// User execute-never.
pub const UXN: u64 = 1 << 54;

/// Caller-facing leaf permissions.
pub const WRITABLE: u32 = 1 << 0;
/// Caller-facing leaf permissions.
pub const EXECUTABLE: u32 = 1 << 1;
/// Map with the device attribute (MAIR index 0).
pub const DEVICE: u32 = 1 << 2;

/// Maximum physical width supported by the 48-bit descriptor layout.
pub const AARCH64_MAX_PHYSICAL_BITS: u32 = 48;

/// MAIR_EL1: attr0 = device-nGnRnE (`0x00`), attr1 = normal write-back (`0xFF`).
pub const MAIR: u64 = 0xFF00;

/// TCR_EL1 with 4 KiB granules, `T0SZ = T1SZ = 16`, inner-shareable write-back walks and the
/// given intermediate physical address size.
pub fn tcr(physical_bits: u32) -> u64 {
    let ips = match physical_bits {
        0..=32 => 0u64,
        33..=36 => 1,
        37..=40 => 2,
        41..=42 => 3,
        43..=44 => 4,
        45..=48 => 5,
        _ => 6,
    };
    const T0SZ: u64 = 16;
    const T1SZ: u64 = 16;
    T0SZ
        | (T1SZ << 16)
        | (0b01 << 8)  // IRGN0: write-back, write-allocate
        | (0b01 << 10) // ORGN0: write-back, write-allocate
        | (0b11 << 12) // SH0: inner shareable; TG0 = 0b00 (4 KiB) is implicit
        | (0b01 << 24) // IRGN1
        | (0b01 << 26) // ORGN1
        | (0b11 << 28) // SH1: inner shareable
        | (0b10 << 30) // TG1: 4 KiB
        | (ips << 32)
}

/// The two translation-table roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roots {
    /// Identity-mapped loader root.
    pub ttbr0: u64,
    /// HHDM and kernel root.
    pub ttbr1: u64,
}

/// A pair of aarch64 translation tables.
pub struct PageTables<M: PhysMem> {
    memory: M,
    ttbr0: u64,
    ttbr1: u64,
    physical_bits: u32,
}

impl<M: PhysMem> PageTables<M> {
    /// Create both roots with the architectural 48-bit physical width.
    pub fn new(memory: M) -> Result<Self, PageError> {
        Self::new_with_physical_bits(memory, AARCH64_MAX_PHYSICAL_BITS)
    }

    /// Create both roots with an explicit physical address width.
    pub fn new_with_physical_bits(mut memory: M, physical_bits: u32) -> Result<Self, PageError> {
        if !(MIN_PHYSICAL_BITS..=AARCH64_MAX_PHYSICAL_BITS).contains(&physical_bits) {
            return Err(PageError::InvalidAddress);
        }
        let ttbr0 = memory.allocate_frame()?;
        let ttbr1 = memory.allocate_frame()?;
        if !physical_ok(ttbr0, physical_bits) || !physical_ok(ttbr1, physical_bits) {
            return Err(PageError::InvalidAddress);
        }
        if !memory.zero_frame(ttbr0) || !memory.zero_frame(ttbr1) {
            return Err(PageError::InvalidAddress);
        }
        Ok(Self {
            memory,
            ttbr0,
            ttbr1,
            physical_bits,
        })
    }

    /// Physical address of the identity-mapped root.
    pub fn ttbr0(&self) -> u64 {
        self.ttbr0
    }

    /// Physical address of the HHDM/kernel root.
    pub fn ttbr1(&self) -> u64 {
        self.ttbr1
    }

    /// True when `vaddr` already has a valid translation.
    pub fn is_mapped(&self, vaddr: u64) -> bool {
        let root = if vaddr >> 48 == 0xffff {
            self.ttbr1
        } else {
            self.ttbr0
        };
        translate(&self.memory, root, vaddr).is_some()
    }

    /// Consume the builder and return the frame pool plus both roots.
    pub fn into_parts(self) -> (M, Roots) {
        (
            self.memory,
            Roots {
                ttbr0: self.ttbr0,
                ttbr1: self.ttbr1,
            },
        )
    }

    /// Consume the builder and return only the roots.
    pub fn into_roots(self) -> Roots {
        Roots {
            ttbr0: self.ttbr0,
            ttbr1: self.ttbr1,
        }
    }

    /// Identity-map `len` bytes at physical address `paddr` through TTBR0.
    pub fn map_identity(&mut self, paddr: u64, len: u64, flags: u32) -> Result<(), PageError> {
        self.map_range_4k(false, paddr, paddr, len, flags)
    }

    /// Map `len` bytes at a high virtual address through TTBR1.
    pub fn map_kernel_range(
        &mut self,
        vaddr: u64,
        paddr: u64,
        len: u64,
        flags: u32,
    ) -> Result<(), PageError> {
        self.map_range_4k(true, vaddr, paddr, len, flags)
    }

    /// Map a physical range into the HHDM using 2 MiB blocks where both ends are aligned.
    pub fn map_hhdm_region(
        &mut self,
        hhdm_base: u64,
        paddr: u64,
        len: u64,
        flags: u32,
    ) -> Result<(), PageError> {
        let mut vaddr = hhdm_base
            .checked_add(paddr)
            .ok_or(PageError::InvalidAddress)?;
        let mut paddr = paddr;
        let mut left = len;
        while left > 0 {
            if left >= PAGE_SIZE_2M
                && vaddr.is_multiple_of(PAGE_SIZE_2M)
                && paddr.is_multiple_of(PAGE_SIZE_2M)
            {
                self.map_leaf(true, vaddr, paddr, flags, true)?;
                vaddr = vaddr
                    .checked_add(PAGE_SIZE_2M)
                    .ok_or(PageError::InvalidAddress)?;
                paddr = paddr
                    .checked_add(PAGE_SIZE_2M)
                    .ok_or(PageError::InvalidAddress)?;
                left = left.saturating_sub(PAGE_SIZE_2M);
            } else {
                self.map_leaf(true, vaddr, paddr, flags, false)?;
                vaddr = vaddr
                    .checked_add(PAGE_SIZE)
                    .ok_or(PageError::InvalidAddress)?;
                paddr = paddr
                    .checked_add(PAGE_SIZE)
                    .ok_or(PageError::InvalidAddress)?;
                left = left.saturating_sub(PAGE_SIZE);
            }
        }
        Ok(())
    }

    fn map_range_4k(
        &mut self,
        high: bool,
        vaddr: u64,
        paddr: u64,
        len: u64,
        flags: u32,
    ) -> Result<(), PageError> {
        if !paddr.is_multiple_of(PAGE_SIZE) || !vaddr.is_multiple_of(PAGE_SIZE) {
            return Err(PageError::InvalidAddress);
        }
        let mut vaddr = vaddr;
        let mut paddr = paddr;
        let mut left = len;
        while left > 0 {
            self.map_leaf(high, vaddr, paddr, flags, false)?;
            vaddr = vaddr
                .checked_add(PAGE_SIZE)
                .ok_or(PageError::InvalidAddress)?;
            paddr = paddr
                .checked_add(PAGE_SIZE)
                .ok_or(PageError::InvalidAddress)?;
            left = left.saturating_sub(PAGE_SIZE);
        }
        Ok(())
    }

    fn map_leaf(
        &mut self,
        high: bool,
        vaddr: u64,
        paddr: u64,
        flags: u32,
        block: bool,
    ) -> Result<(), PageError> {
        if !canonical(vaddr, high)
            || !self.valid_physical(paddr)
            || !vaddr.is_multiple_of(PAGE_SIZE)
            || !paddr.is_multiple_of(PAGE_SIZE)
        {
            return Err(PageError::InvalidAddress);
        }
        if block && (!vaddr.is_multiple_of(PAGE_SIZE_2M) || !paddr.is_multiple_of(PAGE_SIZE_2M)) {
            return Err(PageError::InvalidAddress);
        }
        check_flags(flags)?;

        let root = if high { self.ttbr1 } else { self.ttbr0 };
        let l1_table = self.ensure_table(root, index(vaddr, 39))?;
        let l2_table = self.ensure_table(l1_table, index(vaddr, 30))?;
        if block {
            let slot = entry_slot(l2_table, index(vaddr, 21))?;
            if self.read(slot)? & VALID != 0 {
                return Err(PageError::AlreadyMapped);
            }
            self.write(slot, leaf(paddr, flags, true))
        } else {
            let l3_table = self.ensure_table(l2_table, index(vaddr, 21))?;
            let slot = entry_slot(l3_table, index(vaddr, 12))?;
            if self.read(slot)? & VALID != 0 {
                return Err(PageError::AlreadyMapped);
            }
            self.write(slot, leaf(paddr, flags, false))
        }
    }

    fn ensure_table(&mut self, parent: u64, slot_index: usize) -> Result<u64, PageError> {
        let slot = entry_slot(parent, slot_index)?;
        let entry = self.read(slot)?;
        if entry & VALID != 0 {
            if entry & TABLE_OR_PAGE == 0 {
                return Err(PageError::HugePage);
            }
            let address = entry & ADDRESS_MASK;
            if !self.valid_physical(address) {
                return Err(PageError::InvalidAddress);
            }
            return Ok(address);
        }
        let frame = self.memory.allocate_frame()?;
        if !self.valid_physical(frame) {
            return Err(PageError::InvalidAddress);
        }
        if !self.memory.zero_frame(frame) {
            return Err(PageError::InvalidAddress);
        }
        self.write(slot, frame | VALID | TABLE_OR_PAGE)?;
        Ok(frame)
    }

    fn valid_physical(&self, paddr: u64) -> bool {
        physical_ok(paddr, self.physical_bits)
    }

    fn read(&self, physical: u64) -> Result<u64, PageError> {
        self.memory
            .read_u64(physical)
            .ok_or(PageError::InvalidAddress)
    }

    fn write(&mut self, physical: u64, value: u64) -> Result<(), PageError> {
        if self.memory.write_u64(physical, value) {
            Ok(())
        } else {
            Err(PageError::InvalidAddress)
        }
    }
}

/// Descriptor for a 4 KiB page (`block == false`) or a 2 MiB block (`block == true`).
pub fn leaf(paddr: u64, flags: u32, block: bool) -> u64 {
    let mut descriptor = paddr & ADDRESS_MASK;
    descriptor |= VALID | AF | SH_INNER;
    descriptor |= if flags & DEVICE != 0 {
        ATTR_DEVICE
    } else {
        ATTR_NORMAL
    };
    if flags & EXECUTABLE != 0 {
        // Execute-only is not used: read+execute with UXN set and PXN clear.
        descriptor |= AP_RO | UXN;
    } else if flags & WRITABLE != 0 {
        descriptor |= AP_RW | PXN | UXN;
    } else {
        descriptor |= AP_RO | PXN | UXN;
    }
    if !block {
        descriptor |= TABLE_OR_PAGE;
    }
    descriptor
}

fn canonical(vaddr: u64, high: bool) -> bool {
    let top = vaddr >> 48;
    if high { top == 0xffff } else { top == 0 }
}

fn check_flags(flags: u32) -> Result<(), PageError> {
    if flags & !(WRITABLE | EXECUTABLE | DEVICE) != 0 {
        return Err(PageError::InvalidAddress);
    }
    if flags & WRITABLE != 0 && flags & EXECUTABLE != 0 {
        return Err(PageError::WritableAndExecutable);
    }
    Ok(())
}

fn physical_ok(paddr: u64, bits: u32) -> bool {
    (MIN_PHYSICAL_BITS..=MAX_PHYSICAL_BITS).contains(&bits)
        && paddr.is_multiple_of(PAGE_SIZE)
        && paddr >> bits == 0
}

fn index(vaddr: u64, shift: u32) -> usize {
    ((vaddr >> shift) & 0x1ff) as usize
}

fn entry_slot(table: u64, index: usize) -> Result<u64, PageError> {
    if index >= 512 {
        return Err(PageError::InvalidAddress);
    }
    table
        .checked_add((index as u64) * 8)
        .ok_or(PageError::InvalidAddress)
}

/// Walk `root` and translate `vaddr`, returning `(paddr, leaf descriptor without the address)`.
pub fn translate<M: PhysMem>(memory: &M, root: u64, vaddr: u64) -> Option<(u64, u64)> {
    if !canonical(vaddr, vaddr >> 48 == 0xffff) {
        return None;
    }
    let l0 = memory.read_u64(entry_slot(root, index(vaddr, 39)).ok()?)?;
    if l0 & VALID == 0 || l0 & TABLE_OR_PAGE == 0 {
        return None;
    }
    let l1 = memory.read_u64(entry_slot(l0 & ADDRESS_MASK, index(vaddr, 30)).ok()?)?;
    if l1 & VALID == 0 || l1 & TABLE_OR_PAGE == 0 {
        return None;
    }
    let l2 = memory.read_u64(entry_slot(l1 & ADDRESS_MASK, index(vaddr, 21)).ok()?)?;
    if l2 & VALID == 0 {
        return None;
    }
    if l2 & TABLE_OR_PAGE == 0 {
        return Some((
            (l2 & ADDRESS_MASK) + (vaddr & (PAGE_SIZE_2M - 1)),
            l2 & !ADDRESS_MASK,
        ));
    }
    let l3 = memory.read_u64(entry_slot(l2 & ADDRESS_MASK, index(vaddr, 12)).ok()?)?;
    if l3 & VALID == 0 {
        return None;
    }
    Some((
        (l3 & ADDRESS_MASK) + (vaddr & (PAGE_SIZE - 1)),
        l3 & !ADDRESS_MASK,
    ))
}

/// Build the aarch64 loader tables.
#[cfg(target_os = "uefi")]
pub fn build_loader_tables(map: crate::paging_core::LoaderMap<'_>) -> Result<Roots, PageError> {
    use raven_bootinfo::MemoryKind;

    let crate::paging_core::LoaderMap {
        pool,
        regions,
        kernel,
        image_base,
        image_size,
        old_stack_start,
        old_stack_end,
        stack_base_phys,
        stack_size,
    } = map;
    let pool_base = pool.base();
    let pool_size = pool.size();
    let physical_bits = crate::handoff::physical_bits().min(AARCH64_MAX_PHYSICAL_BITS);
    let mut tables = PageTables::new_with_physical_bits(pool, physical_bits)?;

    for region in regions {
        match region.kind() {
            Some(MemoryKind::Mmio) => tables.map_hhdm_region(
                crate::layout::HHDM_BASE,
                region.base,
                region.len,
                WRITABLE | DEVICE,
            )?,
            Some(MemoryKind::BadMemory) => {}
            _ => tables.map_hhdm_region(
                crate::layout::HHDM_BASE,
                region.base,
                region.len,
                WRITABLE,
            )?,
        }
    }

    // QEMU AAVMF does not describe the PL011 frame in the UEFI memory map, so the test kernel's
    // HHDM serial access needs an explicit device mapping. This is the platform UART address from
    // the S12 contract.
    const PL011_PHYS: u64 = 0x0900_0000;
    if !tables.is_mapped(crate::layout::HHDM_BASE + PL011_PHYS) {
        tables.map_hhdm_region(
            crate::layout::HHDM_BASE,
            PL011_PHYS,
            PAGE_SIZE,
            WRITABLE | DEVICE,
        )?;
    }

    for segment in &kernel.segments {
        let page_vaddr = segment.vaddr & !(PAGE_SIZE - 1);
        let page_paddr = segment.paddr & !(PAGE_SIZE - 1);
        let length = (segment.vaddr & (PAGE_SIZE - 1))
            .checked_add(segment.len)
            .and_then(|value| {
                value
                    .checked_add(PAGE_SIZE - 1)
                    .map(|rounded| rounded & !(PAGE_SIZE - 1))
            })
            .ok_or(PageError::InvalidAddress)?;
        let flags = if segment.prot & 0b010 != 0 {
            WRITABLE
        } else if segment.prot & 0b100 != 0 {
            EXECUTABLE
        } else {
            0
        };
        tables.map_kernel_range(page_vaddr, page_paddr, length, flags)?;
    }

    let image_start = image_base & !(PAGE_SIZE - 1);
    let image_length = (image_base & (PAGE_SIZE - 1))
        .checked_add(image_size)
        .and_then(|value| {
            value
                .checked_add(PAGE_SIZE - 1)
                .map(|rounded| rounded & !(PAGE_SIZE - 1))
        })
        .ok_or(PageError::InvalidAddress)?;
    // The loader image is read-only and executable; after the table switch only its code runs.
    tables.map_identity(image_start, image_length, EXECUTABLE)?;
    tables.map_identity(
        old_stack_start,
        old_stack_end.saturating_sub(old_stack_start),
        WRITABLE,
    )?;
    tables.map_identity(pool_base, pool_size, WRITABLE)?;
    // Identity-map the kernel copy only for the `dc cvau` clean before the jump.
    tables.map_identity(kernel.phys_base & !(PAGE_SIZE - 1), kernel.size, WRITABLE)?;
    // `stack_base_phys` is part of the UEFI LOADER_DATA map above, so it is already in the HHDM;
    // keep the parameter referenced so the shared LoaderMap stays architecture-neutral.
    let _ = (stack_base_phys, stack_size);

    Ok(tables.into_roots())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    struct FakeMem {
        bytes: vec::Vec<u8>,
        base: u64,
        next: u64,
        allocated: usize,
    }

    impl FakeMem {
        fn new(frames: usize) -> Self {
            Self {
                bytes: vec![0; frames * PAGE_SIZE as usize],
                base: 0x10_0000,
                next: 0x10_0000,
                allocated: 0,
            }
        }

        fn index(&self, physical: u64) -> Option<usize> {
            let offset = physical.checked_sub(self.base)?;
            let index = usize::try_from(offset).ok()?;
            (index < self.bytes.len()).then_some(index)
        }
    }

    impl PhysMem for FakeMem {
        fn allocate_frame(&mut self) -> Result<u64, PageError> {
            let frame = self.next;
            let next = frame.checked_add(PAGE_SIZE).ok_or(PageError::OutOfFrames)?;
            if self.index(next.saturating_sub(1)).is_none() {
                return Err(PageError::OutOfFrames);
            }
            self.next = next;
            self.allocated += 1;
            if !self.zero_frame(frame) {
                return Err(PageError::InvalidAddress);
            }
            Ok(frame)
        }

        fn read_u64(&self, physical: u64) -> Option<u64> {
            let index = self.index(physical)?;
            let bytes: [u8; 8] = self.bytes.get(index..index + 8)?.try_into().ok()?;
            Some(u64::from_ne_bytes(bytes))
        }

        fn write_u64(&mut self, physical: u64, value: u64) -> bool {
            let Some(index) = self.index(physical) else {
                return false;
            };
            let Some(slot) = self.bytes.get_mut(index..index + 8) else {
                return false;
            };
            slot.copy_from_slice(&value.to_ne_bytes());
            true
        }

        fn zero_frame(&mut self, physical: u64) -> bool {
            let Some(index) = self.index(physical) else {
                return false;
            };
            let Some(frame) = self.bytes.get_mut(index..index + PAGE_SIZE as usize) else {
                return false;
            };
            frame.fill(0);
            true
        }
    }

    const HHDM: u64 = 0xFFFF_8000_0000_0000;
    const KERNEL: u64 = 0xFFFF_FFFF_8020_0000;

    fn tables(frames: usize) -> PageTables<FakeMem> {
        PageTables::new_with_physical_bits(FakeMem::new(frames), 44).unwrap()
    }

    fn leaf_at(tables: &PageTables<FakeMem>, vaddr: u64) -> u64 {
        let root = if vaddr >> 48 == 0xffff {
            tables.ttbr1
        } else {
            tables.ttbr0
        };
        let l1 = tables
            .memory
            .read_u64(entry_slot(root, index(vaddr, 39)).unwrap())
            .unwrap();
        let l2 = tables
            .memory
            .read_u64(entry_slot(l1 & ADDRESS_MASK, index(vaddr, 30)).unwrap())
            .unwrap();
        let l3 = tables
            .memory
            .read_u64(entry_slot(l2 & ADDRESS_MASK, index(vaddr, 21)).unwrap())
            .unwrap();
        tables
            .memory
            .read_u64(entry_slot(l3 & ADDRESS_MASK, index(vaddr, 12)).unwrap())
            .unwrap()
    }

    #[test]
    fn tcr_encodes_granules_sizes_and_cache_attributes() {
        let value = tcr(44);
        assert_eq!(value & 0x3f, 16, "T0SZ");
        assert_eq!((value >> 16) & 0x3f, 16, "T1SZ");
        assert_eq!((value >> 14) & 0x3, 0, "TG0 = 4 KiB");
        assert_eq!((value >> 30) & 0x3, 0b10, "TG1 = 4 KiB");
        assert_eq!((value >> 8) & 0x3, 0b01, "IRGN0");
        assert_eq!((value >> 10) & 0x3, 0b01, "ORGN0");
        assert_eq!((value >> 12) & 0x3, 0b11, "SH0");
        assert_eq!((value >> 24) & 0x3, 0b01, "IRGN1");
        assert_eq!((value >> 26) & 0x3, 0b01, "ORGN1");
        assert_eq!((value >> 28) & 0x3, 0b11, "SH1");
        assert_eq!((value >> 32) & 0x7, 4, "IPS 44-bit");
    }

    #[test]
    fn mair_matches_the_contract_attributes() {
        assert_eq!(MAIR & 0xff, 0x00, "attr0 device-nGnRnE");
        assert_eq!((MAIR >> 8) & 0xff, 0xff, "attr1 normal write-back");
        assert_eq!(MAIR, 0xff00);
    }

    #[test]
    fn page_descriptor_encoding_for_executable_text() {
        let mut tables = tables(16);
        tables
            .map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, EXECUTABLE)
            .unwrap();
        let descriptor = leaf_at(&tables, KERNEL);
        assert_eq!(descriptor & VALID, VALID);
        assert_eq!(descriptor & TABLE_OR_PAGE, TABLE_OR_PAGE);
        assert_eq!(descriptor & AF, AF);
        assert_eq!(descriptor & SH_INNER, SH_INNER);
        assert_eq!(descriptor & (0b11 << 6), AP_RO);
        assert_eq!(descriptor & PXN, 0);
        assert_eq!(descriptor & UXN, UXN);
        assert_eq!(descriptor & (0b111 << 2), ATTR_NORMAL);
    }

    #[test]
    fn writable_data_is_rw_and_nonexecute() {
        let mut tables = tables(16);
        tables
            .map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, WRITABLE)
            .unwrap();
        let descriptor = leaf_at(&tables, KERNEL);
        assert_eq!(descriptor & (0b11 << 6), AP_RW);
        assert_eq!(descriptor & PXN, PXN);
        assert_eq!(descriptor & UXN, UXN);
    }

    #[test]
    fn read_only_rodata_is_ro_and_nonexecute() {
        let mut tables = tables(16);
        tables
            .map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, 0)
            .unwrap();
        let descriptor = leaf_at(&tables, KERNEL);
        assert_eq!(descriptor & (0b11 << 6), AP_RO);
        assert_eq!(descriptor & PXN, PXN);
        assert_eq!(descriptor & UXN, UXN);
    }

    #[test]
    fn device_uses_attribute_index_zero() {
        let mut tables = tables(16);
        tables
            .map_hhdm_region(HHDM, 0x0900_0000, PAGE_SIZE, WRITABLE | DEVICE)
            .unwrap();
        let (_, flags) = translate(&tables.memory, tables.ttbr1, HHDM + 0x0900_0000).unwrap();
        assert_eq!(flags & (0b111 << 2), ATTR_DEVICE);
        assert_eq!(flags & SH_INNER, SH_INNER);
    }

    #[test]
    fn normal_memory_uses_attribute_index_one() {
        let mut tables = tables(16);
        tables
            .map_hhdm_region(HHDM, 0x20_0000, PAGE_SIZE, WRITABLE)
            .unwrap();
        let (_, flags) = translate(&tables.memory, tables.ttbr1, HHDM + 0x20_0000).unwrap();
        assert_eq!(flags & (0b111 << 2), ATTR_NORMAL);
    }

    #[test]
    fn two_megabyte_block_clears_bit_one() {
        let mut tables = tables(16);
        tables
            .map_hhdm_region(HHDM, 0x20_0000, PAGE_SIZE_2M, WRITABLE)
            .unwrap();
        let (paddr, flags) = translate(&tables.memory, tables.ttbr1, HHDM + 0x20_0000).unwrap();
        assert_eq!(paddr, 0x20_0000);
        assert_eq!(flags & TABLE_OR_PAGE, 0);
        assert_eq!(flags & AF, AF);
    }

    #[test]
    fn writable_and_executable_leaf_is_rejected() {
        let mut tables = tables(8);
        assert_eq!(
            tables.map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, WRITABLE | EXECUTABLE),
            Err(PageError::WritableAndExecutable)
        );
    }

    #[test]
    fn misaligned_address_is_rejected() {
        let mut tables = tables(8);
        assert_eq!(
            tables.map_kernel_range(KERNEL + 1, 0x20_0000, PAGE_SIZE, 0),
            Err(PageError::InvalidAddress)
        );
        assert_eq!(
            tables.map_kernel_range(KERNEL, 0x20_1001, PAGE_SIZE, 0),
            Err(PageError::InvalidAddress)
        );
    }

    #[test]
    fn noncanonical_virtual_address_is_rejected() {
        let mut tables = tables(8);
        assert_eq!(
            tables.map_kernel_range(0x0000_8000_0000_0000, 0x20_0000, PAGE_SIZE, 0),
            Err(PageError::InvalidAddress)
        );
        assert_eq!(
            tables.map_identity(0xFFFF_8000_0000_0000, PAGE_SIZE, 0),
            Err(PageError::InvalidAddress)
        );
    }

    #[test]
    fn duplicate_mapping_is_rejected() {
        let mut tables = tables(16);
        tables
            .map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, 0)
            .unwrap();
        assert_eq!(
            tables.map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, 0),
            Err(PageError::AlreadyMapped)
        );
    }

    #[test]
    fn identity_map_translates_low_addresses() {
        let mut tables = tables(16);
        tables.map_identity(0x1000_0000, 0x4000, WRITABLE).unwrap();
        let (paddr, _) = translate(&tables.memory, tables.ttbr0, 0x1000_2000).unwrap();
        assert_eq!(paddr, 0x1000_2000);
    }

    #[test]
    fn kernel_mapping_translates_high_addresses() {
        let mut tables = tables(16);
        tables
            .map_kernel_range(KERNEL, 0x20_0000, 0x3000, 0)
            .unwrap();
        let (paddr, _) = translate(&tables.memory, tables.ttbr1, KERNEL + 0x2000).unwrap();
        assert_eq!(paddr, 0x20_2000);
    }

    #[test]
    fn ttbr0_and_ttbr1_are_distinct_roots() {
        let tables = tables(8);
        assert_ne!(tables.ttbr0(), tables.ttbr1());
        let (_memory, roots) = tables.into_parts();
        assert_ne!(roots.ttbr0, roots.ttbr1);
    }

    #[test]
    fn physical_width_is_explicit() {
        let mut tables = PageTables::new_with_physical_bits(FakeMem::new(8), 40).unwrap();
        assert_eq!(
            tables.map_kernel_range(KERNEL, 1u64 << 40, PAGE_SIZE, 0),
            Err(PageError::InvalidAddress)
        );
        assert!(PageTables::new_with_physical_bits(FakeMem::new(4), 11).is_err());
        assert!(PageTables::new_with_physical_bits(FakeMem::new(4), 49).is_err());
    }

    #[test]
    fn unmapped_address_is_none() {
        let mut tables = tables(8);
        tables
            .map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, 0)
            .unwrap();
        assert!(translate(&tables.memory, tables.ttbr1, KERNEL + PAGE_SIZE).is_none());
        assert!(translate(&tables.memory, tables.ttbr1, 0x4000_0000).is_none());
    }

    #[test]
    fn adjacent_block_and_page_coexist() {
        let mut tables = tables(32);
        tables
            .map_hhdm_region(HHDM, 0x20_0000, PAGE_SIZE_2M, WRITABLE)
            .unwrap();
        tables
            .map_hhdm_region(HHDM, 0x40_0000, PAGE_SIZE, WRITABLE)
            .unwrap();
        assert_eq!(
            translate(&tables.memory, tables.ttbr1, HHDM + 0x20_0000)
                .unwrap()
                .0,
            0x20_0000
        );
        assert_eq!(
            translate(&tables.memory, tables.ttbr1, HHDM + 0x40_0000)
                .unwrap()
                .0,
            0x40_0000
        );
    }
}
