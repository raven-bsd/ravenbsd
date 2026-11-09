// SPDX-License-Identifier: MIT
// Copyright (c) 2017 Jeremy Soller
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A view of one 4 KiB page table reached through the direct map.

use crate::arch::x86_64::{PAGE_ENTRIES, PAGE_ENTRY_SHIFT, PAGE_SHIFT};
use crate::{Entry, PhysAddr, PhysMemory, VirtAddr};

/// One page table at a known level (0 = PT, 3 = PML4).
#[derive(Clone, Copy, Debug)]
pub struct PageTable {
    phys: PhysAddr,
    level: u8,
}

impl PageTable {
    /// Build a table view.
    ///
    /// `level` must be 0..=3 and `phys` must be a 4 KiB-aligned frame.
    pub const fn new(phys: PhysAddr, level: u8) -> Self {
        Self { phys, level }
    }

    /// Physical address of the table.
    pub const fn phys(self) -> PhysAddr {
        self.phys
    }

    /// Paging level.
    pub const fn level(self) -> u8 {
        self.level
    }

    /// Index selected by `virt` in this level.
    pub const fn index_of(self, virt: VirtAddr) -> usize {
        let shift = PAGE_SHIFT + (self.level as usize) * PAGE_ENTRY_SHIFT;
        ((virt.data() >> shift) as usize) & (PAGE_ENTRIES - 1)
    }

    /// Read entry `index`, or `None` when the direct map cannot reach the table.
    pub fn entry<M: PhysMemory>(self, memory: &M, index: usize) -> Option<Entry> {
        if index >= PAGE_ENTRIES {
            return None;
        }
        let pointer = self.entry_pointer(memory, index)?;
        // SAFETY: `PhysMemory` guarantees the table frame is mapped writable and readable; the
        // index is in bounds and an entry is 8 bytes at an 8-byte-aligned address.
        Some(Entry::from_raw(unsafe {
            core::ptr::read_volatile(pointer.cast_const())
        }))
    }

    /// Write entry `index`; returns false when the index or direct map is invalid.
    pub fn set_entry<M: PhysMemory>(self, memory: &M, index: usize, entry: Entry) -> bool {
        if index >= PAGE_ENTRIES {
            return false;
        }
        let Some(pointer) = self.entry_pointer(memory, index) else {
            return false;
        };
        // SAFETY: `PhysMemory` guarantees the table frame is mapped writable; the index is in
        // bounds and the pointer is 8-byte aligned.
        unsafe {
            core::ptr::write_volatile(pointer, entry.raw());
        }
        true
    }

    /// Descend into the child table at `index`.
    pub fn next<M: PhysMemory>(self, memory: &M, index: usize) -> Option<Self> {
        if self.level == 0 {
            return None;
        }
        let entry = self.entry(memory, index)?;
        if !entry.is_present() || entry.is_huge() {
            return None;
        }
        let child = entry.address();
        if !child.0.is_multiple_of(crate::PAGE_SIZE) {
            return None;
        }
        Some(Self::new(child, self.level - 1))
    }

    /// True when no entry in this table is present.
    pub fn is_empty<M: PhysMemory>(self, memory: &M) -> bool {
        (0..PAGE_ENTRIES).all(|index| {
            self.entry(memory, index)
                .is_none_or(|entry| !entry.is_present())
        })
    }

    fn entry_pointer<M: PhysMemory>(self, memory: &M, index: usize) -> Option<*mut u64> {
        let base = memory.phys_to_virt(self.phys);
        if base.is_null() {
            return None;
        }
        // SAFETY: the direct-map base points at a 4 KiB frame; the offset stays inside it
        // because `index < PAGE_ENTRIES` and each entry is 8 bytes.
        Some(unsafe { base.add(index * core::mem::size_of::<u64>()).cast::<u64>() })
    }
}

const _: () = assert!(PAGE_ENTRIES == 512);
