// SPDX-License-Identifier: MIT
// Copyright (c) 2017 Jeremy Soller
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One 8-byte page-table entry.

use crate::arch::x86_64::{ENTRY_ADDRESS_MASK, ENTRY_FLAGS_MASK, PAGE_SHIFT};
use crate::{PageFlags, PhysAddr};

/// The present bit.
pub const ENTRY_PRESENT: u64 = 1 << 0;
/// The huge-page bit (PS) at PDP and PD levels.
pub const ENTRY_HUGE: u64 = 1 << 7;

/// A raw x86-64 page-table entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Entry(u64);

impl Entry {
    /// Build an entry pointing at `phys` with `flags`.
    pub const fn new(phys: PhysAddr, flags: PageFlags) -> Self {
        let address = ((phys.0 >> PAGE_SHIFT) & ENTRY_ADDRESS_MASK) << PAGE_SHIFT;
        Self(address | flags.bits())
    }

    /// Wrap raw entry bits.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw entry bits.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// True when the present bit is set.
    pub const fn is_present(self) -> bool {
        self.0 & ENTRY_PRESENT != 0
    }

    /// True when the huge-page bit is set.
    pub const fn is_huge(self) -> bool {
        self.0 & ENTRY_HUGE != 0
    }

    /// The physical address in this entry, valid when the entry is present.
    pub const fn address(self) -> PhysAddr {
        PhysAddr(((self.0 >> PAGE_SHIFT) & ENTRY_ADDRESS_MASK) << PAGE_SHIFT)
    }

    /// The flags in this entry.
    pub const fn flags(self) -> PageFlags {
        PageFlags::from_bits(self.0 & ENTRY_FLAGS_MASK)
    }

    /// Replace the flags while keeping the address and huge-page bit.
    pub fn set_flags(&mut self, flags: PageFlags) {
        let huge = if self.is_huge() { ENTRY_HUGE } else { 0 };
        self.0 = (self.0 & !ENTRY_FLAGS_MASK) | flags.bits() | huge;
    }
}

const _: () = assert!(core::mem::size_of::<Entry>() == 8);
