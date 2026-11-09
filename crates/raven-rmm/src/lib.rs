// SPDX-License-Identifier: MIT
// Copyright (c) 2017 Jeremy Soller
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Page-table mapping for RavenBSD.
//!
//! This crate is a port and trim of Redox's `rmm` for x86-64 four-level paging. The original
//! architecture-generic table, entry and mapper machinery is kept; the buddy/bump allocators,
//! the emulated architecture and the non-x86 ports are dropped. Callers supply physical memory
//! access, frame allocation and TLB flushing through the three traits below so the mapper is
//! host-testable.
//!
//! TODO(S14): port the aarch64 and riscv64 backends when those architectures land.

#![no_std]

pub mod arch;
pub mod page;

pub use arch::x86_64::{
    ENTRY_ADDRESS_WIDTH, PAGE_ENTRIES, PAGE_ENTRY_SHIFT, PAGE_LEVELS, PAGE_SHIFT, PAGE_SIZE,
    PHYS_ADDR_MASK, entry_address, entry_flags, is_canonical,
};
pub use page::{Entry, Mapper, PageFlags, PageSize, PageTable};

use core::fmt;

/// A physical address.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct PhysAddr(pub u64);

impl PhysAddr {
    /// Build a physical address.
    pub const fn new(address: u64) -> Self {
        Self(address)
    }

    /// The numeric address.
    pub const fn data(self) -> u64 {
        self.0
    }

    /// True when the address fits the 52-bit physical-address field of a 4-level PTE.
    pub const fn is_encodable(self) -> bool {
        self.0 <= PHYS_ADDR_MASK
    }

    /// Add `offset`, returning `None` on overflow.
    pub const fn checked_add(self, offset: u64) -> Option<Self> {
        match self.0.checked_add(offset) {
            Some(address) => Some(Self(address)),
            None => None,
        }
    }
}

/// A virtual address.
///
/// Constructors reject non-canonical values; `new_unchecked` exists only for addresses the
/// caller knows are canonical, such as linker symbols.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct VirtAddr(u64);

impl VirtAddr {
    /// Build a canonical virtual address.
    pub const fn new(address: u64) -> Option<Self> {
        if is_canonical(address) {
            Some(Self(address))
        } else {
            None
        }
    }

    /// Build a virtual address without checking canonicality.
    ///
    /// # Safety
    ///
    /// `address` must be canonical for the active paging mode.
    pub const unsafe fn new_unchecked(address: u64) -> Self {
        Self(address)
    }

    /// The numeric address.
    pub const fn data(self) -> u64 {
        self.0
    }

    /// True for canonical x86-64 four-level addresses.
    pub const fn is_canonical(self) -> bool {
        is_canonical(self.0)
    }

    /// True in the upper (kernel) canonical half.
    pub const fn is_kernel(self) -> bool {
        (self.0 as i64) < 0
    }

    /// Add `offset`, returning `None` on overflow or when the result is non-canonical.
    pub const fn checked_add(self, offset: u64) -> Option<Self> {
        match self.0.checked_add(offset) {
            Some(address) => Self::new(address),
            None => None,
        }
    }
}

/// A source of zeroed page-table frames.
pub trait FrameAllocator {
    /// Allocate one 4 KiB frame. The frame contents are not trusted.
    fn allocate(&mut self) -> Option<PhysAddr>;

    /// Return a frame previously returned by [`FrameAllocator::allocate`].
    fn free(&mut self, frame: PhysAddr);
}

/// Access to physical memory through a direct map.
///
/// Implementations must return a non-null, writable pointer to the 4 KiB frame at `phys` for the
/// lifetime of the mapper. The page-table walker reads these frames, so writes through the
/// pointer must be visible to the CPU (a normal cached mapping is fine).
pub trait PhysMemory {
    /// Translate a physical address to a pointer in the direct map.
    fn phys_to_virt(&self, phys: PhysAddr) -> *mut u8;
}

/// Invalidate TLB entries after a mapping change.
pub trait TlbFlusher {
    /// Invalidate one virtual page.
    fn flush(&mut self, virt: VirtAddr);

    /// Invalidate the whole TLB.
    fn flush_all(&mut self);
}

/// Why a mapping operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapError {
    /// A virtual or physical address is outside the encodable range.
    NotCanonical,
    /// The address does not meet the alignment required by the page size.
    Misaligned,
    /// A page is already mapped at the target.
    AlreadyMapped,
    /// No page is mapped at the target.
    NotMapped,
    /// The requested flags would make a page writable and executable.
    WriteExecute,
    /// The frame allocator ran out of frames.
    OutOfFrames,
    /// A huge page or a subtable conflicts with the requested mapping.
    HugePageConflict,
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::NotCanonical => f.write_str("address is not encodable"),
            MapError::Misaligned => f.write_str("address is not aligned for the page size"),
            MapError::AlreadyMapped => f.write_str("page is already mapped"),
            MapError::NotMapped => f.write_str("page is not mapped"),
            MapError::WriteExecute => f.write_str("refusing a writable executable page"),
            MapError::OutOfFrames => f.write_str("out of page-table frames"),
            MapError::HugePageConflict => f.write_str("huge page conflicts with the mapping"),
        }
    }
}

/// Round `value` down to a page boundary.
pub const fn page_align_down(value: u64) -> u64 {
    value & !(PAGE_SIZE - 1)
}

/// Round `value` up to a page boundary, returning `None` on overflow.
pub const fn page_align_up(value: u64) -> Option<u64> {
    match value.checked_add(PAGE_SIZE - 1) {
        Some(rounded) => Some(rounded & !(PAGE_SIZE - 1)),
        None => None,
    }
}
