// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The page-descriptor encoding trait.
//!
//! The mapper, table and entry types are architecture-neutral; everything that touches raw
//! descriptor bits lives behind [`Encoding`] so x86-64 PTEs and aarch64 descriptors share one
//! walker. [`X86_64Encoding`] reproduces the S06 behaviour bit for bit; [`Aarch64Encoding`]
//! implements the ARMv8-A 4 KiB-granule format (the applied architectural facts are documented in
//! `arch/aarch64.rs`).
//!
//! `PageFlags` is a *logical* flag set: the x86-64 encoding passes its bits straight into the PTE
//! (they are the architectural PTE bits), while the aarch64 encoding interprets them and builds a
//! descriptor with the correct `AP`/`PXN`/`UXN`/`AttrIndx`/`nG` fields. Raw logical bits are never
//! copied into an aarch64 descriptor.

use crate::PhysAddr;
use crate::page::{PageFlags, PageSize};

/// A page-descriptor encoding.
pub trait Encoding {
    /// Number of paging levels (x86-64: PML4..PT; aarch64: L0..L3; both 4).
    const PAGE_LEVELS: usize;

    /// Highest encodable physical address (inclusive mask).
    const PHYS_ADDR_MASK: u64;

    /// Build a leaf descriptor for `phys` with `flags` at granularity `size`.
    ///
    /// Granularity above 4 KiB (a block/huge descriptor) clears the table/page bit on aarch64 and
    /// sets PS on x86-64.
    fn leaf(phys: PhysAddr, flags: PageFlags, size: PageSize) -> u64;

    /// Build a sub-table descriptor pointing at `phys`; `user` marks a user-owned subtree where
    /// the architecture has a per-table user bit (x86-64 only).
    fn table(phys: PhysAddr, user: bool) -> u64;

    /// Split a leaf descriptor at `level` into its address, logical flags and granularity.
    fn decode(entry: u64, level: u8) -> (PhysAddr, PageFlags, PageSize);

    /// True when `entry` at `level > 0` points at a sub-table rather than a block.
    fn is_table(entry: u64, level: u8) -> bool;

    /// True when `entry` is present/valid.
    fn is_present(entry: u64) -> bool;

    /// The canonical-address predicate for this architecture.
    fn is_canonical(address: u64) -> bool;
}

/// The x86-64 PTE encoding: the historical S06 behaviour, bit for bit.
pub enum X86_64Encoding {}

/// The aarch64 descriptor encoding.
pub enum Aarch64Encoding {}

impl Encoding for X86_64Encoding {
    const PAGE_LEVELS: usize = crate::arch::x86_64::PAGE_LEVELS;
    const PHYS_ADDR_MASK: u64 = crate::arch::x86_64::PHYS_ADDR_MASK;

    fn leaf(phys: PhysAddr, flags: PageFlags, size: PageSize) -> u64 {
        let address = ((phys.0 >> crate::arch::x86_64::PAGE_SHIFT)
            & crate::arch::x86_64::ENTRY_ADDRESS_MASK)
            << crate::arch::x86_64::PAGE_SHIFT;
        let present = flags.union(PageFlags::from_bits(crate::page::ENTRY_PRESENT));
        match size {
            PageSize::Size4K => address | present.bits(),
            _ => address | present.bits() | crate::page::ENTRY_HUGE,
        }
    }

    fn table(phys: PhysAddr, user: bool) -> u64 {
        let address = ((phys.0 >> crate::arch::x86_64::PAGE_SHIFT)
            & crate::arch::x86_64::ENTRY_ADDRESS_MASK)
            << crate::arch::x86_64::PAGE_SHIFT;
        let mut flags = crate::page::ENTRY_PRESENT | crate::page::WRITABLE;
        if user {
            flags |= crate::page::USER;
        }
        address | flags
    }

    fn decode(entry: u64, level: u8) -> (PhysAddr, PageFlags, PageSize) {
        let address = crate::arch::x86_64::entry_address(entry);
        let flags = PageFlags::from_bits(crate::arch::x86_64::entry_flags(entry));
        let size = page_size_for_level(level);
        (PhysAddr(address), flags, size)
    }

    fn is_table(entry: u64, level: u8) -> bool {
        level > 0 && entry & crate::page::ENTRY_PRESENT != 0 && entry & crate::page::ENTRY_HUGE == 0
    }

    fn is_present(entry: u64) -> bool {
        entry & crate::page::ENTRY_PRESENT != 0
    }

    fn is_canonical(address: u64) -> bool {
        crate::arch::x86_64::is_canonical(address)
    }
}

impl Encoding for Aarch64Encoding {
    const PAGE_LEVELS: usize = crate::arch::aarch64::PAGE_LEVELS;
    const PHYS_ADDR_MASK: u64 = crate::arch::aarch64::PHYS_ADDR_MASK;

    fn leaf(phys: PhysAddr, flags: PageFlags, size: PageSize) -> u64 {
        // Descriptor base: the output address, the valid bit, the access flag (pre-set so no
        // access-flag fault occurs) and inner shareability (required for SMP-safe tables).
        let mut descriptor = (phys.0 & (crate::arch::aarch64::ENTRY_ADDRESS_MASK << 12))
            | crate::arch::aarch64::ENTRY_VALID
            | crate::arch::aarch64::AF
            | crate::arch::aarch64::SH_INNER;

        // Attribute index. `mmio()` carries the cache-disable + write-through logical bits (the
        // ticket's "NO_CACHE" case) and selects attr3 device-nGnRE; cache-disable alone selects
        // attr0 device-nGnRnE, which is how the loader maps the memory map's MMIO regions;
        // `NON_CACHEABLE` selects attr2 for future DMA buffers.
        let attribute = if flags.contains(PageFlags::from_bits(crate::page::NON_CACHEABLE)) {
            crate::arch::aarch64::ATTR_NON_CACHEABLE
        } else if flags.contains(PageFlags::mmio_attribute()) {
            crate::arch::aarch64::ATTR_DEVICE_NGNRE
        } else if flags.contains(PageFlags::from_bits(crate::page::CACHE_DISABLE)) {
            crate::arch::aarch64::ATTR_DEVICE
        } else {
            crate::arch::aarch64::ATTR_NORMAL
        };
        descriptor |= attribute;

        // Access permission: USER pages use the EL0+EL1 AP encodings, kernel pages the EL1-only
        // ones; AP[0] distinguishes writable from read-only.
        if flags.is_user() {
            descriptor |= if flags.is_writable() {
                crate::arch::aarch64::AP_ALL_RW
            } else {
                crate::arch::aarch64::AP_ALL_RO
            };
        } else {
            descriptor |= if flags.is_writable() {
                crate::arch::aarch64::AP_EL1_RW
            } else {
                crate::arch::aarch64::AP_EL1_RO
            };
        }

        // Execution: aarch64 has separate privilege XN bits. Kernel pages clear PXN and always
        // set UXN (user code never executes kernel text); user pages clear UXN and always set PXN
        // (privileged code never executes user code). The opposite bit is always set, so W^X and
        // privilege separation are both structural.
        if flags.is_user() {
            descriptor |= crate::arch::aarch64::PXN;
            if !flags.is_executable() {
                descriptor |= crate::arch::aarch64::UXN;
            }
        } else {
            descriptor |= crate::arch::aarch64::UXN;
            if !flags.is_executable() {
                descriptor |= crate::arch::aarch64::PXN;
            }
        }

        // nG: kernel pages are global (nG clear, ASID-independent), user pages are per-ASID.
        if flags.is_user() {
            descriptor |= crate::arch::aarch64::NG;
        }

        match size {
            // L3 descriptors must have bit 1 set; L1/L2 blocks leave it clear.
            PageSize::Size4K => descriptor | crate::arch::aarch64::ENTRY_TABLE_OR_PAGE,
            _ => descriptor,
        }
    }

    fn table(phys: PhysAddr, _user: bool) -> u64 {
        // Table descriptors carry only valid + table bits; the aarch64 format has no per-table
        // user bit, and PXNTable/UXNTable (bits 59/60) stay clear.
        (phys.0 & (crate::arch::aarch64::ENTRY_ADDRESS_MASK << 12))
            | crate::arch::aarch64::ENTRY_VALID
            | crate::arch::aarch64::ENTRY_TABLE_OR_PAGE
    }

    fn decode(entry: u64, level: u8) -> (PhysAddr, PageFlags, PageSize) {
        let address = crate::arch::aarch64::entry_address(entry);
        let mut flags = PageFlags::empty();

        // Attribute index back to logical flags.
        let attribute = entry & (0b111 << 2);
        if attribute == crate::arch::aarch64::ATTR_NON_CACHEABLE {
            flags.insert(PageFlags::from_bits(crate::page::NON_CACHEABLE));
        } else if attribute == crate::arch::aarch64::ATTR_DEVICE_NGNRE {
            flags.insert(PageFlags::mmio_attribute());
        } else if attribute == crate::arch::aarch64::ATTR_DEVICE {
            flags.insert(PageFlags::from_bits(crate::page::CACHE_DISABLE));
        }

        // AP back to user/writable, XN back to the logical no-execute bit, nG back to global.
        let ap = (entry >> 6) & 0b11;
        let user = ap == 0b01 || ap == 0b11;
        let writable = ap == 0b00 || ap == 0b01;
        if user {
            flags.insert(PageFlags::from_bits(crate::page::USER));
        }
        if writable {
            flags.insert(PageFlags::from_bits(crate::page::WRITABLE));
        }
        let executable = if user {
            entry & crate::arch::aarch64::UXN == 0
        } else {
            entry & crate::arch::aarch64::PXN == 0
        };
        if !executable {
            flags.insert(PageFlags::from_bits(crate::page::NO_EXECUTE));
        }
        if entry & crate::arch::aarch64::NG == 0 {
            flags.insert(PageFlags::from_bits(crate::page::GLOBAL));
        }

        // Level 0 is always a 4 KiB page; decode is only called on leaves, so a level-1/2 entry is
        // a block of the level's size.
        let size = page_size_for_level(level);
        (PhysAddr(address), flags, size)
    }

    fn is_table(entry: u64, level: u8) -> bool {
        level > 0
            && entry & crate::arch::aarch64::ENTRY_VALID != 0
            && entry & crate::arch::aarch64::ENTRY_TABLE_OR_PAGE != 0
    }

    fn is_present(entry: u64) -> bool {
        entry & crate::arch::aarch64::ENTRY_VALID != 0
    }

    fn is_canonical(address: u64) -> bool {
        crate::arch::aarch64::is_canonical(address)
    }
}

/// Granularity of a leaf at `level` when the descriptor shape does not say (x86-64) or when the
/// caller only knows the level.
fn page_size_for_level(level: u8) -> PageSize {
    match level {
        0 => PageSize::Size4K,
        1 => PageSize::Size2M,
        _ => PageSize::Size1G,
    }
}
