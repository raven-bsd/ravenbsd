// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The page-table mapper: map, unmap, protect, translate and address-space creation.
//!
//! The walker is encoding-neutral: every raw-descriptor decision goes through [`Encoding`], so
//! the same code drives x86-64 PTEs and aarch64 descriptors. The design follows the NetBSD pmap
//! rules for table ownership: new intermediate tables are allocated unlinked, the leaf is
//! written first, and the table chain is linked top-down; any failure frees the unlinked frames
//! and leaves no reachable empty table.
//!
//! Huge-page splitting is not implemented. Mapping a 4 KiB page over a 2 MiB/1 GiB page (or the
//! reverse) returns [`MapError::HugePageConflict`]; callers must unmap the huge mapping first.
//! Empty intermediate tables created by the mapper are freed on [`Mapper::unmap`]. That rule is
//! only safe for tables owned by this mapper: a user address-space mapper must only unmap user
//! addresses, because its kernel-half root entries are shared with the kernel root.

use crate::page::encoding::{Encoding, X86_64Encoding};
use crate::page::{Entry, PageFlags, PageSize, PageTable};
use crate::{FrameAllocator, MapError, PhysAddr, PhysMemory, TlbFlusher, VirtAddr, page_align_up};

/// One intermediate table allocated by `map`, waiting to be linked.
#[derive(Clone, Copy)]
struct NewTable {
    parent: PhysAddr,
    parent_level: u8,
    parent_index: usize,
    frame: PhysAddr,
}

/// The at-most-three unlinked tables of one `map` call.
struct PendingTables {
    slots: [Option<NewTable>; 3],
}

impl PendingTables {
    const fn new() -> Self {
        Self { slots: [None; 3] }
    }

    fn push(&mut self, table: NewTable) -> bool {
        for slot in &mut self.slots {
            if slot.is_none() {
                *slot = Some(table);
                return true;
            }
        }
        false
    }

    fn iter(&self) -> impl Iterator<Item = &NewTable> {
        self.slots.iter().flatten()
    }

    fn free<A: FrameAllocator>(&self, allocator: &mut A) {
        for table in self.iter() {
            allocator.free(table.frame);
        }
    }

    fn clear_links<M: PhysMemory>(&self, memory: &M, linked: usize) -> bool {
        let mut cleared = true;
        for table in self.slots[..linked].iter().rev().flatten() {
            cleared &= PageTable::new(table.parent, table.parent_level).set_entry(
                memory,
                table.parent_index,
                Entry::from_raw(0),
            );
        }
        cleared
    }
}

/// A page-table mapper over a caller-provided root table.
pub struct Mapper<'a, A: FrameAllocator, M: PhysMemory, T: TlbFlusher, E: Encoding = X86_64Encoding>
{
    root: PhysAddr,
    allocator: &'a mut A,
    memory: &'a M,
    tlb: &'a mut T,
    encoding: core::marker::PhantomData<E>,
}

impl<'a, A: FrameAllocator, M: PhysMemory, T: TlbFlusher> Mapper<'a, A, M, T, X86_64Encoding> {
    /// Build a mapper for `root`.
    ///
    /// Fails with [`MapError::Misaligned`] when the root is not page aligned, and with
    /// [`MapError::NotCanonical`] when it is outside the encodable physical range.
    pub fn new(
        root: PhysAddr,
        allocator: &'a mut A,
        memory: &'a M,
        tlb: &'a mut T,
    ) -> Result<Self, MapError> {
        Self::with_encoding(root, allocator, memory, tlb)
    }
}

impl<'a, A: FrameAllocator, M: PhysMemory, T: TlbFlusher, E: Encoding> Mapper<'a, A, M, T, E> {
    /// Build a mapper for `root` with an explicit descriptor encoding.
    ///
    /// Same failure conditions as [`Mapper::new`].
    pub fn with_encoding(
        root: PhysAddr,
        allocator: &'a mut A,
        memory: &'a M,
        tlb: &'a mut T,
    ) -> Result<Self, MapError> {
        if !root.0.is_multiple_of(crate::PAGE_SIZE) {
            return Err(MapError::Misaligned);
        }
        if root.0 > E::PHYS_ADDR_MASK {
            return Err(MapError::NotCanonical);
        }
        Ok(Self {
            root,
            allocator,
            memory,
            tlb,
            encoding: core::marker::PhantomData,
        })
    }

    /// The root frame.
    pub const fn root(&self) -> PhysAddr {
        self.root
    }

    /// The frame allocator.
    pub fn allocator_mut(&mut self) -> &mut A {
        self.allocator
    }

    /// Flush the whole TLB.
    pub fn flush_all(&mut self) {
        self.tlb.flush_all();
    }

    /// Map one page.
    pub fn map(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageFlags,
        size: PageSize,
    ) -> Result<(), MapError> {
        self.check_leaf_request(virt, flags, size)?;
        if phys.0 > E::PHYS_ADDR_MASK || !E::is_canonical(virt.data()) {
            return Err(MapError::NotCanonical);
        }
        let align = size.bytes();
        if !virt.data().is_multiple_of(align) || !phys.0.is_multiple_of(align) {
            return Err(MapError::Misaligned);
        }

        let target_level = size.level();
        let mut pending = PendingTables::new();
        let mut table = PageTable::new(self.root, (E::PAGE_LEVELS - 1) as u8);
        loop {
            let index = table.index_of(virt);
            let Some(entry) = table.entry(self.memory, index) else {
                pending.free(self.allocator);
                return Err(MapError::OutOfFrames);
            };

            if table.level() == target_level {
                if E::is_present(entry.raw()) {
                    pending.free(self.allocator);
                    if target_level > 0 && E::is_table(entry.raw(), target_level) {
                        return Err(MapError::HugePageConflict);
                    }
                    return Err(MapError::AlreadyMapped);
                }
                if !table.set_entry(
                    self.memory,
                    index,
                    Entry::from_raw(E::leaf(phys, flags, size)),
                ) {
                    pending.free(self.allocator);
                    return Err(MapError::OutOfFrames);
                }
                break;
            }

            if E::is_present(entry.raw()) && !E::is_table(entry.raw(), table.level()) {
                pending.free(self.allocator);
                return Err(MapError::HugePageConflict);
            }

            if E::is_present(entry.raw()) {
                let child = entry.address();
                if !child.0.is_multiple_of(crate::PAGE_SIZE) {
                    pending.free(self.allocator);
                    return Err(MapError::HugePageConflict);
                }
                table = PageTable::new(child, table.level() - 1);
                continue;
            }

            let frame = match self.allocator.allocate() {
                Some(frame) => frame,
                None => {
                    pending.free(self.allocator);
                    return Err(MapError::OutOfFrames);
                }
            };
            if !frame.0.is_multiple_of(crate::PAGE_SIZE) || frame.0 > E::PHYS_ADDR_MASK {
                self.allocator.free(frame);
                pending.free(self.allocator);
                return Err(MapError::NotCanonical);
            }
            if let Err(error) = self.zero_frame(frame) {
                self.allocator.free(frame);
                pending.free(self.allocator);
                return Err(error);
            }
            let parent = table;
            if !pending.push(NewTable {
                parent: parent.phys(),
                parent_level: parent.level(),
                parent_index: index,
                frame,
            }) {
                self.allocator.free(frame);
                pending.free(self.allocator);
                return Err(MapError::OutOfFrames);
            }
            table = PageTable::new(frame, parent.level() - 1);
        }

        // Link the chain top-down after the leaf is in place. A failure clears the links made so
        // far; only when every clear succeeds are the frames returned to the allocator.
        let user = flags.is_user();
        for (linked, new_table) in pending.iter().enumerate() {
            let parent = PageTable::new(new_table.parent, new_table.parent_level);
            if !parent.set_entry(
                self.memory,
                new_table.parent_index,
                Entry::from_raw(E::table(new_table.frame, user)),
            ) {
                if pending.clear_links(self.memory, linked) {
                    pending.free(self.allocator);
                }
                return Err(MapError::OutOfFrames);
            }
        }
        self.tlb.flush(virt);
        Ok(())
    }

    /// Unmap the page at `virt` and return its physical base.
    pub fn unmap(&mut self, virt: VirtAddr) -> Result<PhysAddr, MapError> {
        let (table, index, entry, _size) = self.locate(virt).ok_or(MapError::NotMapped)?;
        if !E::is_present(entry.raw()) {
            return Err(MapError::NotMapped);
        }
        let phys = entry.address();
        if !table.set_entry(self.memory, index, Entry::from_raw(0)) {
            return Err(MapError::NotMapped);
        }
        self.tlb.flush(virt);
        self.prune(virt);
        Ok(phys)
    }

    /// Translate `virt` to its physical page base, flags and page size.
    pub fn translate(&mut self, virt: VirtAddr) -> Option<(PhysAddr, PageFlags, PageSize)> {
        let (_table, _index, entry, size) = self.locate(virt)?;
        if !E::is_present(entry.raw()) {
            return None;
        }
        let level = size.level();
        let (phys, flags, size) = E::decode(entry.raw(), level);
        Some((phys, flags, size))
    }

    /// Change the flags of the mapped page at `virt`.
    pub fn protect(&mut self, virt: VirtAddr, flags: PageFlags) -> Result<(), MapError> {
        if flags.is_write_execute() {
            return Err(MapError::WriteExecute);
        }
        if flags.is_user() && !E::is_user_half(virt.data()) {
            return Err(MapError::UserKernelSplit);
        }
        let (table, index, entry, size) = self.locate(virt).ok_or(MapError::NotMapped)?;
        if !E::is_present(entry.raw()) {
            return Err(MapError::NotMapped);
        }
        // Rebuild the descriptor from the address and the level's granularity so the shape
        // (huge/block vs page) and the present bits survive the flag change.
        let updated = Entry::from_raw(E::leaf(entry.address(), flags, size));
        if !table.set_entry(self.memory, index, updated) {
            return Err(MapError::NotMapped);
        }
        self.tlb.flush(virt);
        Ok(())
    }

    /// Map `len` bytes, preferring 2 MiB pages where both addresses and the length allow it.
    ///
    /// On error, mappings created earlier in the range are not rolled back.
    pub fn map_range(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        len: u64,
        flags: PageFlags,
    ) -> Result<(), MapError> {
        if flags.is_write_execute() {
            return Err(MapError::WriteExecute);
        }
        if len == 0 {
            return Ok(());
        }
        let len = page_align_up(len).ok_or(MapError::NotCanonical)?;
        let end = virt.data().checked_add(len).ok_or(MapError::NotCanonical)?;
        if !E::is_canonical(virt.data()) || !E::is_canonical(end - 1) {
            return Err(MapError::NotCanonical);
        }
        let phys_end = phys.0.checked_add(len).ok_or(MapError::NotCanonical)?;
        if phys_end > E::PHYS_ADDR_MASK.saturating_add(1) {
            return Err(MapError::NotCanonical);
        }

        let mut current_virt = virt.data();
        let mut current_phys = phys.0;
        let mut remaining = len;
        while remaining > 0 {
            let size = if remaining >= PageSize::Size2M.bytes()
                && current_virt.is_multiple_of(PageSize::Size2M.bytes())
                && current_phys.is_multiple_of(PageSize::Size2M.bytes())
            {
                PageSize::Size2M
            } else {
                PageSize::Size4K
            };
            let step = size.bytes().min(remaining);
            if step < size.bytes() {
                return Err(MapError::Misaligned);
            }
            let page = VirtAddr::new_raw(current_virt).ok_or(MapError::NotCanonical)?;
            self.map(page, PhysAddr(current_phys), flags, size)?;
            current_virt = current_virt
                .checked_add(step)
                .ok_or(MapError::NotCanonical)?;
            current_phys = current_phys
                .checked_add(step)
                .ok_or(MapError::NotCanonical)?;
            remaining -= step;
        }
        Ok(())
    }

    /// Create a new address space whose upper root half is copied from `copy_kernel_half_from`.
    ///
    /// The upper half is entries 256..512: bit 47 of the virtual address selects TTBR0 vs TTBR1
    /// on aarch64 and the user vs kernel half on x86-64, so the same split applies to both.
    pub fn new_address_space(
        &mut self,
        copy_kernel_half_from: PhysAddr,
    ) -> Result<PhysAddr, MapError> {
        if !copy_kernel_half_from.0.is_multiple_of(crate::PAGE_SIZE)
            || copy_kernel_half_from.0 > E::PHYS_ADDR_MASK
        {
            return Err(MapError::Misaligned);
        }
        let root = self.allocator.allocate().ok_or(MapError::OutOfFrames)?;
        if !root.0.is_multiple_of(crate::PAGE_SIZE) || root.0 > E::PHYS_ADDR_MASK {
            self.allocator.free(root);
            return Err(MapError::NotCanonical);
        }
        if let Err(error) = self.zero_frame(root) {
            self.allocator.free(root);
            return Err(error);
        }
        let source = match self.frame_pointer(copy_kernel_half_from) {
            Ok(pointer) => pointer,
            Err(error) => {
                self.allocator.free(root);
                return Err(error);
            }
        };
        let destination = match self.frame_pointer(root) {
            Ok(pointer) => pointer,
            Err(error) => {
                self.allocator.free(root);
                return Err(error);
            }
        };
        for index in crate::PAGE_ENTRIES / 2..crate::PAGE_ENTRIES {
            // SAFETY: both pointers refer to 4 KiB page-table frames returned by the direct map;
            // `index` is in bounds and entries are 8-byte aligned.
            let value = unsafe { core::ptr::read_volatile(source.add(index)) };
            // SAFETY: same invariant for the freshly zeroed destination frame.
            unsafe {
                core::ptr::write_volatile(destination.add(index), value);
            }
        }
        Ok(root)
    }

    fn check_leaf_request(
        &self,
        virt: VirtAddr,
        flags: PageFlags,
        size: PageSize,
    ) -> Result<(), MapError> {
        if flags.is_write_execute() {
            return Err(MapError::WriteExecute);
        }
        if flags.is_user() && !E::is_user_half(virt.data()) {
            return Err(MapError::UserKernelSplit);
        }
        if size == PageSize::Size1G {
            return Err(MapError::HugePageConflict);
        }
        Ok(())
    }

    fn locate(&self, virt: VirtAddr) -> Option<(PageTable, usize, Entry, PageSize)> {
        let mut table = PageTable::new(self.root, (E::PAGE_LEVELS - 1) as u8);
        loop {
            let index = table.index_of(virt);
            let entry = table.entry(self.memory, index)?;
            if !E::is_present(entry.raw()) {
                return Some((table, index, entry, page_size_for_level(table.level())));
            }
            if !E::is_table(entry.raw(), table.level()) {
                return Some((table, index, entry, page_size_for_level(table.level())));
            }
            if table.level() == 0 {
                return Some((table, index, entry, PageSize::Size4K));
            }
            table = PageTable::new(entry.address(), table.level() - 1);
        }
    }

    fn prune(&mut self, virt: VirtAddr) {
        // path[level] is the entry in a level-`level` table that points at the level-`level - 1`
        // child. A 2 MiB leaf at level 1 leaves path[1] empty but still records path[2]/path[3],
        // so the now-empty level-1 table is reclaimed through its parent.
        let mut path: [Option<(PageTable, usize)>; crate::PAGE_LEVELS] = [None; crate::PAGE_LEVELS];
        let mut table = PageTable::new(self.root, (E::PAGE_LEVELS - 1) as u8);
        while table.level() > 0 {
            let index = table.index_of(virt);
            let Some(entry) = table.entry(self.memory, index) else {
                return;
            };
            if !E::is_present(entry.raw()) || !E::is_table(entry.raw(), table.level()) {
                break;
            }
            path[usize::from(table.level())] = Some((table, index));
            table = PageTable::new(entry.address(), table.level() - 1);
        }

        for (level, slot) in path.iter().enumerate().skip(1) {
            let Some((parent, index)) = *slot else {
                continue;
            };
            let Some(entry) = parent.entry(self.memory, index) else {
                return;
            };
            if !E::is_present(entry.raw()) || !E::is_table(entry.raw(), level as u8) {
                continue;
            }
            let child = PageTable::new(entry.address(), (level - 1) as u8);
            if !child.is_empty(self.memory) {
                return;
            }
            if !parent.set_entry(self.memory, index, Entry::from_raw(0)) {
                return;
            }
            self.allocator.free(child.phys());
            self.tlb.flush(virt);
        }
    }

    fn zero_frame(&self, frame: PhysAddr) -> Result<(), MapError> {
        let pointer = self.memory.phys_to_virt(frame);
        if pointer.is_null() {
            return Err(MapError::OutOfFrames);
        }
        // SAFETY: `PhysMemory` guarantees a writable 4 KiB frame at `frame`.
        unsafe {
            core::ptr::write_bytes(pointer, 0, crate::PAGE_SIZE as usize);
        }
        Ok(())
    }

    fn frame_pointer(&self, frame: PhysAddr) -> Result<*mut u64, MapError> {
        let pointer = self.memory.phys_to_virt(frame);
        if pointer.is_null() {
            return Err(MapError::OutOfFrames);
        }
        Ok(pointer.cast::<u64>())
    }
}

/// Granularity of a leaf at `level`.
fn page_size_for_level(level: u8) -> PageSize {
    match level {
        0 => PageSize::Size4K,
        1 => PageSize::Size2M,
        _ => PageSize::Size1G,
    }
}

const _: () = {
    assert!(crate::arch::x86_64::PAGE_SHIFT == 12);
    assert!(crate::arch::x86_64::PAGE_LEVELS == 4);
    assert!(crate::arch::aarch64::PAGE_LEVELS == 4);
    assert!(crate::arch::x86_64::entry_address((1u64 << 12) | 1) == 1 << 12);
    assert!(crate::arch::x86_64::entry_flags((1u64 << 12) | 1) == 1);
    assert!(crate::page::ENTRY_PRESENT == 1);
    assert!(crate::page::ENTRY_HUGE == 0x80);
};
