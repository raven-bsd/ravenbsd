// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! 4-level x86-64 page tables for the loader handoff.
//!
//! This is the x86-64 implementation of the [`crate::paging_core`] contract. The builder is
//! generic over [`PhysMem`] so the host tests can run it against a `Vec<u8>`-backed fake.

use crate::paging_core::{
    MAX_PHYSICAL_BITS, MIN_PHYSICAL_BITS, PAGE_SIZE, PAGE_SIZE_1G, PAGE_SIZE_2M, PageError, PhysMem,
};

/// Physical-address mask of a page-table entry (bits 12..51).
pub const ADDRESS_MASK: u64 = 0x000F_FFFF_FFFF_F000;
/// Entry is present.
pub const PRESENT: u64 = 1;
/// Entry (or its subtree) is writable.
pub const WRITABLE: u64 = 1 << 1;
/// Entry is a huge-page leaf.
pub const HUGE_PAGE: u64 = 1 << 7;
/// Entry is global (not flushed on CR3 writes).
pub const GLOBAL: u64 = 1 << 8;
/// Entry (or its subtree) is not executable.
pub const NO_EXECUTE: u64 = 1 << 63;

/// Roots loaded into CR3 for the kernel handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roots {
    /// Page-table root physical address.
    pub root: u64,
}

/// One 4-level page-table tree.
pub struct PageTables<M: PhysMem> {
    memory: M,
    root: u64,
    physical_bits: u32,
}

impl<M: PhysMem> PageTables<M> {
    /// Create an empty tree with a fresh root frame, accepting the architectural 52-bit physical
    /// address width. Firmware code should prefer [`PageTables::new_with_physical_bits`].
    pub fn new(memory: M) -> Result<Self, PageError> {
        Self::new_with_physical_bits(memory, MAX_PHYSICAL_BITS)
    }

    /// Create an empty tree with an explicit physical address width.
    pub fn new_with_physical_bits(mut memory: M, physical_bits: u32) -> Result<Self, PageError> {
        if !(MIN_PHYSICAL_BITS..=MAX_PHYSICAL_BITS).contains(&physical_bits) {
            return Err(PageError::InvalidAddress);
        }
        let root = memory.allocate_frame()?;
        if !physical_ok(root, physical_bits) {
            return Err(PageError::InvalidAddress);
        }
        if !memory.zero_frame(root) {
            return Err(PageError::InvalidAddress);
        }
        Ok(Self {
            memory,
            root,
            physical_bits,
        })
    }

    /// Physical address width used for validation.
    pub fn physical_bits(&self) -> u32 {
        self.physical_bits
    }

    /// Physical address of the PML4 root (the value to load into CR3).
    pub fn root(&self) -> u64 {
        self.root
    }

    /// Consume the builder and return the backing memory and the root address.
    pub fn into_parts(self) -> (M, u64) {
        (self.memory, self.root)
    }

    /// Map one 4 KiB page.
    pub fn map_4k(&mut self, vaddr: u64, paddr: u64, flags: u64) -> Result<(), PageError> {
        if !is_canonical_virtual(vaddr)
            || !self.valid_physical(paddr)
            || !vaddr.is_multiple_of(PAGE_SIZE)
            || !paddr.is_multiple_of(PAGE_SIZE)
        {
            return Err(PageError::InvalidAddress);
        }
        check_flags(flags)?;
        if flags & HUGE_PAGE != 0 {
            return Err(PageError::InvalidAddress);
        }

        let pdpt = self.ensure_table(self.root, index(vaddr, 39))?;
        let pd = self.ensure_table(pdpt, index(vaddr, 30))?;
        let pt = self.ensure_table(pd, index(vaddr, 21))?;
        let slot = entry_slot(pt, index(vaddr, 12))?;
        if self.read(slot)? & PRESENT != 0 {
            return Err(PageError::AlreadyMapped);
        }
        self.write(slot, paddr | flags | PRESENT)
    }

    /// Map one 2 MiB page.
    pub fn map_2m(&mut self, vaddr: u64, paddr: u64, flags: u64) -> Result<(), PageError> {
        if !is_canonical_virtual(vaddr)
            || !self.valid_physical(paddr)
            || !vaddr.is_multiple_of(PAGE_SIZE_2M)
            || !paddr.is_multiple_of(PAGE_SIZE_2M)
        {
            return Err(PageError::InvalidAddress);
        }
        check_flags(flags)?;
        if flags & HUGE_PAGE != 0 {
            return Err(PageError::InvalidAddress);
        }

        let pdpt = self.ensure_table(self.root, index(vaddr, 39))?;
        let pd = self.ensure_table(pdpt, index(vaddr, 30))?;
        let slot = entry_slot(pd, index(vaddr, 21))?;
        if self.read(slot)? & PRESENT != 0 {
            return Err(PageError::AlreadyMapped);
        }
        self.write(slot, paddr | flags | PRESENT | HUGE_PAGE)
    }

    /// Map `len` bytes with 4 KiB pages.
    pub fn map_range_4k(
        &mut self,
        vaddr: u64,
        paddr: u64,
        len: u64,
        flags: u64,
    ) -> Result<(), PageError> {
        if !is_canonical_virtual(vaddr)
            || !self.valid_physical(paddr)
            || !vaddr.is_multiple_of(PAGE_SIZE)
        {
            return Err(PageError::InvalidAddress);
        }
        let mut vaddr = vaddr;
        let mut paddr = paddr;
        let mut left = len;
        while left > 0 {
            self.map_4k(vaddr, paddr, flags)?;
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

    /// Map one physical range into the direct map, using 2 MiB pages where both ends are aligned
    /// and 4 KiB pages for the unaligned head and tail.
    pub fn map_hhdm_region(
        &mut self,
        hhdm_base: u64,
        paddr: u64,
        len: u64,
        flags: u64,
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
                self.map_2m(vaddr, paddr, flags)?;
                vaddr = vaddr
                    .checked_add(PAGE_SIZE_2M)
                    .ok_or(PageError::InvalidAddress)?;
                paddr = paddr
                    .checked_add(PAGE_SIZE_2M)
                    .ok_or(PageError::InvalidAddress)?;
                left = left.saturating_sub(PAGE_SIZE_2M);
            } else {
                self.map_4k(vaddr, paddr, flags)?;
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

    fn ensure_table(&mut self, parent: u64, index: usize) -> Result<u64, PageError> {
        let slot = entry_slot(parent, index)?;
        let entry = self.read(slot)?;
        if entry & PRESENT != 0 {
            if entry & HUGE_PAGE != 0 {
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
        self.write(slot, frame | PRESENT | WRITABLE)?;
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

fn physical_ok(paddr: u64, bits: u32) -> bool {
    (MIN_PHYSICAL_BITS..=MAX_PHYSICAL_BITS).contains(&bits)
        && paddr.is_multiple_of(PAGE_SIZE)
        && paddr >> bits == 0
}

/// Whether `vaddr` is canonical for 4-level (48-bit virtual) paging.
pub fn is_canonical_virtual(vaddr: u64) -> bool {
    let top = vaddr >> 47;
    top == 0 || top == 0x1_ffff
}

fn check_flags(flags: u64) -> Result<(), PageError> {
    if flags & ADDRESS_MASK != 0 {
        return Err(PageError::InvalidAddress);
    }
    if flags & WRITABLE != 0 && flags & NO_EXECUTE == 0 {
        return Err(PageError::WritableAndExecutable);
    }
    Ok(())
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

/// Walk `root` and translate `vaddr`, returning `(paddr, leaf flags)`.
pub fn translate<M: PhysMem>(memory: &M, root: u64, vaddr: u64) -> Option<(u64, u64)> {
    if !is_canonical_virtual(vaddr) {
        return None;
    }
    let pml4 = memory.read_u64(entry_slot(root, index(vaddr, 39)).ok()?)?;
    if pml4 & PRESENT == 0 {
        return None;
    }
    let pdpt = memory.read_u64(entry_slot(pml4 & ADDRESS_MASK, index(vaddr, 30)).ok()?)?;
    if pdpt & PRESENT == 0 {
        return None;
    }
    if pdpt & HUGE_PAGE != 0 {
        return Some((
            (pdpt & ADDRESS_MASK) + (vaddr & (PAGE_SIZE_1G - 1)),
            pdpt & !ADDRESS_MASK,
        ));
    }
    let pd = memory.read_u64(entry_slot(pdpt & ADDRESS_MASK, index(vaddr, 21)).ok()?)?;
    if pd & PRESENT == 0 {
        return None;
    }
    if pd & HUGE_PAGE != 0 {
        return Some((
            (pd & ADDRESS_MASK) + (vaddr & (PAGE_SIZE_2M - 1)),
            pd & !ADDRESS_MASK,
        ));
    }
    let pt = memory.read_u64(entry_slot(pd & ADDRESS_MASK, index(vaddr, 12)).ok()?)?;
    if pt & PRESENT == 0 {
        return None;
    }
    Some((
        (pt & ADDRESS_MASK) + (vaddr & (PAGE_SIZE - 1)),
        pt & !ADDRESS_MASK,
    ))
}

/// Build the x86-64 loader page tables: HHDM, kernel segments, identity-mapped loader, stacks,
/// page-table pool and kernel stack.
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
    let mut tables = PageTables::new_with_physical_bits(pool, crate::handoff::physical_bits())?;

    for region in regions {
        match region.kind() {
            Some(MemoryKind::Mmio) => {
                let virtual_address = crate::layout::HHDM_BASE
                    .checked_add(region.base)
                    .ok_or(PageError::InvalidAddress)?;
                tables.map_range_4k(
                    virtual_address,
                    region.base,
                    region.len,
                    WRITABLE | NO_EXECUTE,
                )?;
            }
            Some(MemoryKind::BadMemory) => {}
            _ => tables.map_hhdm_region(
                crate::layout::HHDM_BASE,
                region.base,
                region.len,
                WRITABLE | NO_EXECUTE,
            )?,
        }
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
        let flags = (if segment.prot & 0b010 != 0 {
            WRITABLE
        } else {
            0
        }) | (if segment.prot & 0b100 != 0 {
            0
        } else {
            NO_EXECUTE
        });
        tables.map_range_4k(page_vaddr, page_paddr, length, flags)?;
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
    tables.map_range_4k(image_start, image_start, image_length, 0)?;
    tables.map_range_4k(
        old_stack_start,
        old_stack_start,
        old_stack_end.saturating_sub(old_stack_start),
        WRITABLE | NO_EXECUTE,
    )?;
    tables.map_range_4k(
        stack_base_phys,
        stack_base_phys,
        stack_size,
        WRITABLE | NO_EXECUTE,
    )?;
    tables.map_range_4k(pool_base, pool_base, pool_size, WRITABLE | NO_EXECUTE)?;

    let (_pool_meta, root) = tables.into_parts();
    Ok(Roots { root })
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

        fn allocated(&self) -> usize {
            self.allocated
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

    fn build<F>(frames: usize, fill: F) -> (FakeMem, u64)
    where
        F: FnOnce(&mut PageTables<FakeMem>) -> Result<(), PageError>,
    {
        let mut tables = PageTables::new(FakeMem::new(frames)).unwrap();
        fill(&mut tables).unwrap();
        tables.into_parts()
    }

    #[test]
    fn hhdm_uses_2m_pages() {
        let (memory, root) = build(64, |tables| {
            tables.map_hhdm_region(HHDM, 0x20_0000, 0x20_0000, WRITABLE | NO_EXECUTE)
        });
        let (paddr, flags) = translate(&memory, root, HHDM + 0x20_1234).unwrap();
        assert_eq!(paddr, 0x20_1234);
        assert_eq!(flags & HUGE_PAGE, HUGE_PAGE);
        assert_eq!(flags & WRITABLE, WRITABLE);
        assert_eq!(flags & NO_EXECUTE, NO_EXECUTE);
    }

    #[test]
    fn hhdm_unaligned_tail_uses_4k_pages() {
        let (memory, root) = build(64, |tables| {
            tables.map_hhdm_region(HHDM, 0x1ff000, 0x3000, WRITABLE | NO_EXECUTE)
        });
        assert_eq!(
            translate(&memory, root, HHDM + 0x1ff000).unwrap().0,
            0x1ff000
        );
        assert_eq!(
            translate(&memory, root, HHDM + 0x200000).unwrap().0,
            0x200000
        );
        assert_eq!(
            translate(&memory, root, HHDM + 0x201000).unwrap().0,
            0x201000
        );
    }

    #[test]
    fn kernel_text_is_read_and_execute() {
        let (memory, root) = build(64, |tables| tables.map_4k(KERNEL, 0x20_0000, 0));
        let (paddr, flags) = translate(&memory, root, KERNEL).unwrap();
        assert_eq!(paddr, 0x20_0000);
        assert_eq!(flags & WRITABLE, 0);
        assert_eq!(flags & NO_EXECUTE, 0);
    }

    #[test]
    fn kernel_rodata_is_read_and_nonexecute() {
        let (memory, root) = build(64, |tables| {
            tables.map_4k(KERNEL + 0x3000, 0x20_3000, NO_EXECUTE)
        });
        let (_, flags) = translate(&memory, root, KERNEL + 0x3000).unwrap();
        assert_eq!(flags & WRITABLE, 0);
        assert_eq!(flags & NO_EXECUTE, NO_EXECUTE);
    }

    #[test]
    fn kernel_data_is_read_write_and_nonexecute() {
        let (memory, root) = build(64, |tables| {
            tables.map_4k(KERNEL + 0x6000, 0x20_6000, WRITABLE | NO_EXECUTE)
        });
        let (_, flags) = translate(&memory, root, KERNEL + 0x6000).unwrap();
        assert_eq!(flags & WRITABLE, WRITABLE);
        assert_eq!(flags & NO_EXECUTE, NO_EXECUTE);
    }

    #[test]
    fn writable_and_executable_leaf_is_rejected() {
        let mut tables = PageTables::new(FakeMem::new(8)).unwrap();
        assert_eq!(
            tables.map_4k(KERNEL, 0x20_0000, WRITABLE),
            Err(PageError::WritableAndExecutable)
        );
        assert_eq!(
            tables.map_2m(KERNEL, 0x20_0000, WRITABLE),
            Err(PageError::WritableAndExecutable)
        );
    }

    #[test]
    fn translate_unmapped_is_none() {
        let (memory, root) = build(8, |tables| tables.map_4k(KERNEL, 0x20_0000, 0));
        assert!(translate(&memory, root, KERNEL + 0x1000).is_none());
        assert!(translate(&memory, root, 0x4000_0000).is_none());
    }

    #[test]
    fn identity_map_translates_to_itself() {
        let (memory, root) = build(16, |tables| {
            tables.map_range_4k(0x1000_0000, 0x1000_0000, 0x4000, 0)
        });
        assert_eq!(
            translate(&memory, root, 0x1000_2000).unwrap().0,
            0x1000_2000
        );
    }

    #[test]
    fn table_frame_count_stays_bounded() {
        let (memory, root) = build(64, |tables| {
            tables.map_hhdm_region(HHDM, 0, 0x8000_0000, WRITABLE | NO_EXECUTE)?;
            tables.map_4k(KERNEL, 0x20_0000, 0)?;
            tables.map_4k(KERNEL + 0x1000, 0x20_1000, NO_EXECUTE)?;
            tables.map_range_4k(0x1000_0000, 0x1000_0000, 0x8000, WRITABLE | NO_EXECUTE)
        });
        assert!(memory.allocated() <= 64);
        assert_eq!(
            translate(&memory, root, HHDM + 0x7fff_ffff).unwrap().0,
            0x7fff_ffff
        );
    }

    #[test]
    fn adjacent_2m_and_4k_mappings_coexist() {
        let (memory, root) = build(32, |tables| {
            tables.map_2m(HHDM + 0x20_0000, 0x20_0000, WRITABLE | NO_EXECUTE)?;
            tables.map_4k(HHDM + 0x40_0000, 0x40_0000, WRITABLE | NO_EXECUTE)
        });
        assert_eq!(
            translate(&memory, root, HHDM + 0x20_0000).unwrap().0,
            0x20_0000
        );
        assert_eq!(
            translate(&memory, root, HHDM + 0x40_0000).unwrap().0,
            0x40_0000
        );
    }

    #[test]
    fn duplicate_mapping_is_rejected() {
        let mut tables = PageTables::new(FakeMem::new(16)).unwrap();
        tables.map_4k(KERNEL, 0x20_0000, 0).unwrap();
        assert_eq!(
            tables.map_4k(KERNEL, 0x20_0000, 0),
            Err(PageError::AlreadyMapped)
        );
    }

    #[test]
    fn misaligned_mapping_is_rejected() {
        let mut tables = PageTables::new(FakeMem::new(8)).unwrap();
        assert_eq!(
            tables.map_4k(KERNEL + 1, 0x20_0000, 0),
            Err(PageError::InvalidAddress)
        );
        assert_eq!(
            tables.map_2m(KERNEL, 0x20_1000, WRITABLE | NO_EXECUTE),
            Err(PageError::InvalidAddress)
        );
    }

    #[test]
    fn unwritable_execute_only_pages_are_allowed() {
        let (memory, root) = build(16, |tables| tables.map_4k(KERNEL, 0x20_0000, 0));
        let (_, flags) = translate(&memory, root, KERNEL).unwrap();
        assert_eq!(flags & WRITABLE, 0);
        assert_eq!(flags & NO_EXECUTE, 0);
        assert_eq!(flags & PRESENT, PRESENT);
    }

    #[test]
    fn noncanonical_virtual_addresses_are_rejected() {
        let noncanonical = 0x0000_8000_0000_0000;
        assert!(!is_canonical_virtual(noncanonical));
        let mut tables = PageTables::new(FakeMem::new(8)).unwrap();
        assert_eq!(
            tables.map_4k(noncanonical, 0x20_0000, NO_EXECUTE),
            Err(PageError::InvalidAddress)
        );
        assert_eq!(
            tables.map_2m(noncanonical, 0x20_0000, WRITABLE | NO_EXECUTE),
            Err(PageError::InvalidAddress)
        );
        assert_eq!(
            tables.map_range_4k(noncanonical, 0x20_0000, 0x2000, NO_EXECUTE),
            Err(PageError::InvalidAddress)
        );
    }

    #[test]
    fn invalid_high_physical_address_is_rejected() {
        let mut tables = PageTables::new(FakeMem::new(8)).unwrap();
        assert_eq!(
            tables.map_4k(KERNEL, 1u64 << 52, WRITABLE | NO_EXECUTE),
            Err(PageError::InvalidAddress)
        );
    }

    #[test]
    fn physical_width_is_explicit() {
        let mut tables = PageTables::new_with_physical_bits(FakeMem::new(16), 40).unwrap();
        assert_eq!(
            tables.map_4k(KERNEL, 1u64 << 40, NO_EXECUTE),
            Err(PageError::InvalidAddress)
        );
        assert_eq!(tables.map_4k(KERNEL, 1u64 << 39, NO_EXECUTE), Ok(()));
        assert_eq!(
            tables.map_2m(KERNEL + PAGE_SIZE_2M, 1u64 << 40, WRITABLE | NO_EXECUTE),
            Err(PageError::InvalidAddress)
        );
        assert!(PageTables::new_with_physical_bits(FakeMem::new(4), 11).is_err());
        assert!(PageTables::new_with_physical_bits(FakeMem::new(4), 53).is_err());
    }

    struct BadFrameMem;

    impl PhysMem for BadFrameMem {
        fn allocate_frame(&mut self) -> Result<u64, PageError> {
            Ok(1u64 << 52)
        }
        fn read_u64(&self, _physical: u64) -> Option<u64> {
            None
        }
        fn write_u64(&mut self, _physical: u64, _value: u64) -> bool {
            false
        }
        fn zero_frame(&mut self, _physical: u64) -> bool {
            false
        }
    }

    #[test]
    fn invalid_physmem_table_frame_is_rejected() {
        assert!(matches!(
            PageTables::new(BadFrameMem),
            Err(PageError::InvalidAddress)
        ));
    }

    #[test]
    fn canonical_low_and_high_boundaries_are_accepted() {
        let low = 0x0000_7FFF_FFFF_F000;
        let high = 0xFFFF_FFFF_FFFF_F000;
        assert!(is_canonical_virtual(low));
        assert!(is_canonical_virtual(high));
        let (memory, root) = build(32, |tables| {
            tables.map_4k(low, 0x20_0000, NO_EXECUTE)?;
            tables.map_4k(high, 0x20_1000, NO_EXECUTE)
        });
        assert_eq!(translate(&memory, root, low).unwrap().0, 0x20_0000);
        assert_eq!(translate(&memory, root, high).unwrap().0, 0x20_1000);
        assert!(translate(&memory, root, 0x0000_8000_0000_0000).is_none());
    }

    #[test]
    fn hhdm_overflow_is_rejected() {
        let mut tables = PageTables::new(FakeMem::new(8)).unwrap();
        assert_eq!(
            tables.map_hhdm_region(
                u64::MAX - PAGE_SIZE + 1,
                0x20_0000,
                0x2000,
                WRITABLE | NO_EXECUTE,
            ),
            Err(PageError::InvalidAddress)
        );
        assert_eq!(
            tables.map_hhdm_region(
                0x0000_7FFF_FFFF_0000,
                0x1_0000,
                0x1000,
                WRITABLE | NO_EXECUTE,
            ),
            Err(PageError::InvalidAddress)
        );
    }

    #[test]
    fn range_mapping_validates_every_page() {
        let last_low_page = 0x0000_7FFF_FFFF_F000;
        let mut tables = PageTables::new(FakeMem::new(8)).unwrap();
        assert_eq!(
            tables.map_range_4k(last_low_page, 0x20_0000, 0x2000, NO_EXECUTE),
            Err(PageError::InvalidAddress)
        );
    }
}
