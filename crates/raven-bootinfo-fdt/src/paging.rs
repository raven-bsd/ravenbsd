// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal aarch64 4 KiB-granule page tables for a flat Pi 4 entry stub.
//!
//! The builder is deliberately free of `Result`, `?`, checked-arithmetic helpers, slices and
//! iterators: the Pi code runs it **before the MMU is enabled**, so it must not call an outlined
//! `core` helper (the kernel's long-branch thunks point at the higher-half image). Every operation
//! uses wrapping arithmetic and explicit masks; error paths return `None`/`false`.
//!
//! The builder is generic over [`PhysMem`] so host tests can exercise the exact descriptor bits
//! with a byte-vector fake, while the kernel entry stub supplies a static frame pool and physical
//! memory accessors implemented with inline assembly.

/// Shift from bytes to pages.
pub const PAGE_SHIFT: u32 = 12;
/// Page size in bytes.
pub const PAGE_SIZE: u64 = 1 << PAGE_SHIFT;
/// 2 MiB block size.
pub const PAGE_SIZE_2M: u64 = 1 << 21;
/// Entries in one translation table.
pub const PAGE_ENTRIES: usize = 512;
/// Output-address mask for the 48-bit VA/PA format.
pub const ADDRESS_MASK: u64 = 0x0000_FFFF_FFFF_F000;
/// Descriptor is valid (present).
pub const VALID: u64 = 1 << 0;
/// Table descriptor or L3 page descriptor.
pub const TABLE_OR_PAGE: u64 = 1 << 1;
/// Access flag.
pub const AF: u64 = 1 << 10;
/// Inner-shareable attribute.
pub const SH_INNER: u64 = 0b11 << 8;
/// Attribute index 0: device-nGnRnE (the kernel's MAIR uses this slot).
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

/// Leaf is writable.
pub const WRITABLE: u32 = 1 << 0;
/// Leaf is executable.
pub const EXECUTABLE: u32 = 1 << 1;
/// Leaf uses the device attribute.
pub const DEVICE: u32 = 1 << 2;

/// `MAIR_EL1` value matching the kernel's extended attributes.
///
/// attr0 device-nGnRnE, attr1 normal write-back, attr2 normal non-cacheable, attr3 device-nGnRE.
/// The entry stub only uses attr0/attr1; the kernel later installs the same value.
pub const MAIR: u64 = 0x0444_FF00;

/// Minimum physical width accepted by [`PageTables`].
pub const MIN_PHYSICAL_BITS: u32 = 32;
/// Maximum physical width of the 48-bit descriptor format.
pub const MAX_PHYSICAL_BITS: u32 = 48;

/// Physical memory access and frame allocation used by [`PageTables`].
pub trait PhysMem {
    /// Allocate one 4 KiB frame and return its physical address.
    fn allocate_frame(&mut self) -> Option<u64>;
    /// Zero one 4 KiB frame.
    fn zero_frame(&mut self, physical: u64);
    /// Read one 64-bit table entry.
    fn read_u64(&self, physical: u64) -> u64;
    /// Write one 64-bit table entry.
    fn write_u64(&mut self, physical: u64, value: u64);
}

/// The two translation-table roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roots {
    /// Identity-mapped low root.
    pub ttbr0: u64,
    /// Higher-half/HHDM root.
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
    /// Create both roots for the 48-bit physical format.
    #[inline(always)]
    pub fn new(memory: M) -> Option<Self> {
        Self::new_with_physical_bits(memory, MAX_PHYSICAL_BITS)
    }

    /// Create both roots with an explicit physical address width.
    #[inline(always)]
    pub fn new_with_physical_bits(memory: M, physical_bits: u32) -> Option<Self> {
        let mut tables = Self::new_uninit(memory, physical_bits);
        if tables.init() { Some(tables) } else { None }
    }

    /// Build an uninitialised builder without moving a 40-byte `Option` through the stack.
    ///
    /// The Pi 4 entry path calls this before the MMU is on, where any outlined `memcpy` would
    /// branch into the unmapped higher half. Call [`init`](Self::init) immediately afterwards.
    #[inline(always)]
    pub fn new_uninit(memory: M, physical_bits: u32) -> Self {
        Self {
            memory,
            ttbr0: 0,
            ttbr1: 0,
            physical_bits,
        }
    }

    /// Allocate and zero both translation roots.
    #[inline(always)]
    pub fn init(&mut self) -> bool {
        if self.physical_bits < MIN_PHYSICAL_BITS || self.physical_bits > MAX_PHYSICAL_BITS {
            return false;
        }
        let Some(ttbr0) = self.memory.allocate_frame() else {
            return false;
        };
        let Some(ttbr1) = self.memory.allocate_frame() else {
            return false;
        };
        if !physical_ok(ttbr0, self.physical_bits) || !physical_ok(ttbr1, self.physical_bits) {
            return false;
        }
        self.memory.zero_frame(ttbr0);
        self.memory.zero_frame(ttbr1);
        self.ttbr0 = ttbr0;
        self.ttbr1 = ttbr1;
        true
    }

    /// Physical address of the identity root.
    #[inline(always)]
    pub fn ttbr0(&self) -> u64 {
        self.ttbr0
    }

    /// Physical address of the higher-half root.
    #[inline(always)]
    pub fn ttbr1(&self) -> u64 {
        self.ttbr1
    }

    /// Consume the builder and return the roots.
    #[inline(always)]
    pub fn into_roots(self) -> Roots {
        Roots {
            ttbr0: self.ttbr0,
            ttbr1: self.ttbr1,
        }
    }

    /// Identity-map `len` bytes at `paddr` through TTBR0.
    #[inline(always)]
    pub fn map_identity(&mut self, paddr: u64, len: u64, flags: u32) -> bool {
        self.map_range_4k(false, paddr, paddr, len, flags)
    }

    /// Map `len` bytes at a higher-half `vaddr` through TTBR1.
    #[inline(always)]
    pub fn map_kernel_range(&mut self, vaddr: u64, paddr: u64, len: u64, flags: u32) -> bool {
        self.map_range_4k(true, vaddr, paddr, len, flags)
    }

    /// Map a physical range into the HHDM, using 2 MiB blocks where alignment allows.
    #[inline(always)]
    pub fn map_hhdm_region(&mut self, hhdm_base: u64, paddr: u64, len: u64, flags: u32) -> bool {
        let mut vaddr = hhdm_base.wrapping_add(paddr);
        let mut paddr = paddr;
        let mut left = len.wrapping_add(PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        while left > 0 {
            if left >= PAGE_SIZE_2M
                && vaddr & (PAGE_SIZE_2M - 1) == 0
                && paddr & (PAGE_SIZE_2M - 1) == 0
            {
                if !self.map_leaf(true, vaddr, paddr, flags, true) {
                    return false;
                }
                vaddr = vaddr.wrapping_add(PAGE_SIZE_2M);
                paddr = paddr.wrapping_add(PAGE_SIZE_2M);
                left = left.wrapping_sub(PAGE_SIZE_2M);
            } else {
                if !self.map_leaf(true, vaddr, paddr, flags, false) {
                    return false;
                }
                vaddr = vaddr.wrapping_add(PAGE_SIZE);
                paddr = paddr.wrapping_add(PAGE_SIZE);
                left = left.wrapping_sub(PAGE_SIZE);
            }
        }
        true
    }

    #[inline(always)]
    fn map_range_4k(&mut self, high: bool, vaddr: u64, paddr: u64, len: u64, flags: u32) -> bool {
        if paddr & (PAGE_SIZE - 1) != 0 || vaddr & (PAGE_SIZE - 1) != 0 {
            return false;
        }
        let mut vaddr = vaddr;
        let mut paddr = paddr;
        let mut left = len.wrapping_add(PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        while left > 0 {
            if !self.map_leaf(high, vaddr, paddr, flags, false) {
                return false;
            }
            vaddr = vaddr.wrapping_add(PAGE_SIZE);
            paddr = paddr.wrapping_add(PAGE_SIZE);
            left = left.wrapping_sub(PAGE_SIZE);
        }
        true
    }

    #[inline(always)]
    fn map_leaf(&mut self, high: bool, vaddr: u64, paddr: u64, flags: u32, block: bool) -> bool {
        if !canonical(vaddr, high)
            || !self.valid_physical(paddr)
            || vaddr & (PAGE_SIZE - 1) != 0
            || paddr & (PAGE_SIZE - 1) != 0
        {
            return false;
        }
        if block && (vaddr & (PAGE_SIZE_2M - 1) != 0 || paddr & (PAGE_SIZE_2M - 1) != 0) {
            return false;
        }
        if !check_flags(flags) {
            return false;
        }

        let root = if high { self.ttbr1 } else { self.ttbr0 };
        let Some(l1) = self.ensure_table(root, index(vaddr, 39)) else {
            return false;
        };
        let Some(l2) = self.ensure_table(l1, index(vaddr, 30)) else {
            return false;
        };
        if block {
            let slot = entry_slot(l2, index(vaddr, 21));
            if self.memory.read_u64(slot) & VALID != 0 {
                return false;
            }
            self.memory.write_u64(slot, leaf(paddr, flags, true));
            true
        } else {
            let Some(l3) = self.ensure_table(l2, index(vaddr, 21)) else {
                return false;
            };
            let slot = entry_slot(l3, index(vaddr, 12));
            if self.memory.read_u64(slot) & VALID != 0 {
                return false;
            }
            self.memory.write_u64(slot, leaf(paddr, flags, false));
            true
        }
    }

    #[inline(always)]
    fn ensure_table(&mut self, parent: u64, slot_index: usize) -> Option<u64> {
        let slot = entry_slot(parent, slot_index);
        let entry = self.memory.read_u64(slot);
        if entry & VALID != 0 {
            if entry & TABLE_OR_PAGE == 0 {
                return None;
            }
            let address = entry & ADDRESS_MASK;
            if !self.valid_physical(address) {
                return None;
            }
            return Some(address);
        }
        let frame = self.memory.allocate_frame()?;
        if !self.valid_physical(frame) {
            return None;
        }
        self.memory.zero_frame(frame);
        self.memory.write_u64(slot, frame | VALID | TABLE_OR_PAGE);
        Some(frame)
    }

    #[inline(always)]
    fn valid_physical(&self, paddr: u64) -> bool {
        physical_ok(paddr, self.physical_bits)
    }
}

/// Descriptor for a 4 KiB page or a 2 MiB block.
#[inline(always)]
pub fn leaf(paddr: u64, flags: u32, block: bool) -> u64 {
    let mut descriptor = paddr & ADDRESS_MASK;
    descriptor |= VALID | AF | SH_INNER;
    descriptor |= if flags & DEVICE != 0 {
        ATTR_DEVICE
    } else {
        ATTR_NORMAL
    };
    if flags & EXECUTABLE != 0 {
        // Kernel code: EL1 may execute, EL0 may not.
        descriptor |= AP_RO | UXN;
    } else if flags & WRITABLE != 0 {
        descriptor |= AP_RW | PXN | UXN;
    } else {
        descriptor |= AP_RO | PXN | UXN;
    }
    if block {
        descriptor
    } else {
        descriptor | TABLE_OR_PAGE
    }
}

/// `TCR_EL1` with 4 KiB granules, `T0SZ = T1SZ = 16` and the given physical address width.
#[inline(always)]
pub fn tcr(physical_bits: u32) -> u64 {
    let ips = if physical_bits <= 32 {
        0
    } else if physical_bits <= 36 {
        1
    } else if physical_bits <= 40 {
        2
    } else if physical_bits <= 42 {
        3
    } else if physical_bits <= 44 {
        4
    } else if physical_bits <= 48 {
        5
    } else {
        6
    };
    const T0SZ: u64 = 16;
    const T1SZ: u64 = 16;
    T0SZ | (T1SZ << 16)
        | (0b01 << 8)
        | (0b01 << 10)
        | (0b11 << 12)
        | (0b01 << 24)
        | (0b01 << 26)
        | (0b11 << 28)
        | (0b10 << 30)
        | ((ips as u64) << 32)
}

#[inline(always)]
fn canonical(vaddr: u64, high: bool) -> bool {
    let top = vaddr >> 48;
    if high { top == 0xffff } else { top == 0 }
}

#[inline(always)]
fn check_flags(flags: u32) -> bool {
    if flags & !(WRITABLE | EXECUTABLE | DEVICE) != 0 {
        return false;
    }
    flags & WRITABLE == 0 || flags & EXECUTABLE == 0
}

// Explicit comparisons rather than `RangeInclusive::contains`: the Pi entry path calls this
// before the MMU is on, where an outlined helper would branch into the unmapped higher half.
#[inline(always)]
#[allow(clippy::manual_range_contains)]
fn physical_ok(paddr: u64, bits: u32) -> bool {
    bits >= MIN_PHYSICAL_BITS
        && bits <= MAX_PHYSICAL_BITS
        && paddr & (PAGE_SIZE - 1) == 0
        && paddr >> bits == 0
}

#[inline(always)]
fn index(vaddr: u64, shift: u32) -> usize {
    ((vaddr >> shift) & 0x1ff) as usize
}

#[inline(always)]
fn entry_slot(table: u64, index: usize) -> u64 {
    table.wrapping_add((index as u64) << 3)
}

/// Walk `root` and translate `vaddr` to `(physical, flags-without-address)`. Test/diagnostic only;
/// the pre-MMU entry path never calls it.
#[cfg(test)]
pub fn translate<M: PhysMem>(memory: &M, root: u64, vaddr: u64) -> Option<(u64, u64)> {
    if !canonical(vaddr, vaddr >> 48 == 0xffff) {
        return None;
    }
    let l0 = memory.read_u64(entry_slot(root, index(vaddr, 39)));
    if l0 & VALID == 0 || l0 & TABLE_OR_PAGE == 0 {
        return None;
    }
    let l1 = memory.read_u64(entry_slot(l0 & ADDRESS_MASK, index(vaddr, 30)));
    if l1 & VALID == 0 || l1 & TABLE_OR_PAGE == 0 {
        return None;
    }
    let l2 = memory.read_u64(entry_slot(l1 & ADDRESS_MASK, index(vaddr, 21)));
    if l2 & VALID == 0 {
        return None;
    }
    if l2 & TABLE_OR_PAGE == 0 {
        return Some((
            (l2 & ADDRESS_MASK).wrapping_add(vaddr & (PAGE_SIZE_2M - 1)),
            l2 & !ADDRESS_MASK,
        ));
    }
    let l3 = memory.read_u64(entry_slot(l2 & ADDRESS_MASK, index(vaddr, 12)));
    if l3 & VALID == 0 {
        return None;
    }
    Some((
        (l3 & ADDRESS_MASK).wrapping_add(vaddr & (PAGE_SIZE - 1)),
        l3 & !ADDRESS_MASK,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    struct FakeMem {
        bytes: Vec<u8>,
        base: u64,
        next: u64,
    }

    impl FakeMem {
        fn new(frames: usize) -> Self {
            Self {
                bytes: vec![0; frames * PAGE_SIZE as usize],
                base: 0x10_0000,
                next: 0x10_0000,
            }
        }

        fn index(&self, physical: u64) -> Option<usize> {
            let offset = physical.wrapping_sub(self.base);
            let index = usize::try_from(offset).ok()?;
            (index < self.bytes.len()).then_some(index)
        }
    }

    impl PhysMem for FakeMem {
        fn allocate_frame(&mut self) -> Option<u64> {
            let frame = self.next;
            let next = frame.wrapping_add(PAGE_SIZE);
            self.index(next.wrapping_sub(1))?;
            self.next = next;
            self.zero_frame(frame);
            Some(frame)
        }

        fn zero_frame(&mut self, physical: u64) {
            if let Some(index) = self.index(physical)
                && let Some(frame) = self.bytes.get_mut(index..index + PAGE_SIZE as usize)
            {
                frame.fill(0);
            }
        }

        fn read_u64(&self, physical: u64) -> u64 {
            let Some(index) = self.index(physical) else {
                return 0;
            };
            let Some(bytes) = self.bytes.get(index..index + 8) else {
                return 0;
            };
            let mut word = [0u8; 8];
            word.copy_from_slice(bytes);
            u64::from_ne_bytes(word)
        }

        fn write_u64(&mut self, physical: u64, value: u64) {
            if let Some(index) = self.index(physical)
                && let Some(slot) = self.bytes.get_mut(index..index + 8)
            {
                slot.copy_from_slice(&value.to_ne_bytes());
            }
        }
    }

    const HHDM: u64 = 0xFFFF_8000_0000_0000;
    const KERNEL: u64 = 0xFFFF_FFFF_8020_0000;

    fn tables(frames: usize) -> PageTables<FakeMem> {
        PageTables::new_with_physical_bits(FakeMem::new(frames), 48).expect("tables")
    }

    #[test]
    fn tcr_has_4k_granules_and_16_gigabyte_half() {
        let value = tcr(48);
        assert_eq!(value & 0x3f, 16);
        assert_eq!((value >> 16) & 0x3f, 16);
        assert_eq!((value >> 14) & 0x3, 0);
        assert_eq!((value >> 30) & 0x3, 0b10);
        assert_eq!((value >> 32) & 0x7, 5);
    }

    #[test]
    fn executable_leaf_is_read_only_at_el1() {
        let descriptor = leaf(0x20_0000, EXECUTABLE, false);
        assert_eq!(descriptor & VALID, VALID);
        assert_eq!(descriptor & TABLE_OR_PAGE, TABLE_OR_PAGE);
        assert_eq!(descriptor & AF, AF);
        assert_eq!(descriptor & (0b11 << 6), AP_RO);
        assert_eq!(descriptor & PXN, 0);
        assert_eq!(descriptor & UXN, UXN);
    }

    #[test]
    fn writable_leaf_is_nonexecute() {
        let descriptor = leaf(0x20_0000, WRITABLE, false);
        assert_eq!(descriptor & (0b11 << 6), AP_RW);
        assert_eq!(descriptor & PXN, PXN);
        assert_eq!(descriptor & UXN, UXN);
    }

    #[test]
    fn device_leaf_uses_attr0() {
        let descriptor = leaf(0x0900_0000, WRITABLE | DEVICE, false);
        assert_eq!(descriptor & (0b111 << 2), ATTR_DEVICE);
    }

    #[test]
    fn identity_mapping_translates_low_addresses() {
        let mut tables = tables(16);
        assert!(tables.map_identity(0x1000_0000, 0x4000, WRITABLE));
        let (paddr, _) = translate(&tables.memory, tables.ttbr0, 0x1000_2000).unwrap();
        assert_eq!(paddr, 0x1000_2000);
    }

    #[test]
    fn kernel_mapping_translates_high_addresses() {
        let mut tables = tables(16);
        assert!(tables.map_kernel_range(KERNEL, 0x20_0000, 0x3000, EXECUTABLE));
        let (paddr, _) = translate(&tables.memory, tables.ttbr1, KERNEL + 0x2000).unwrap();
        assert_eq!(paddr, 0x20_2000);
    }

    #[test]
    fn hhdm_block_clears_table_or_page() {
        let mut tables = tables(16);
        assert!(tables.map_hhdm_region(HHDM, 0x20_0000, PAGE_SIZE_2M, WRITABLE | DEVICE));
        let (_, flags) = translate(&tables.memory, tables.ttbr1, HHDM + 0x20_0000).unwrap();
        assert_eq!(flags & TABLE_OR_PAGE, 0);
        assert_eq!(flags & AF, AF);
    }

    #[test]
    fn writable_executable_leaf_is_rejected() {
        let mut tables = tables(8);
        assert!(!tables.map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, WRITABLE | EXECUTABLE));
    }

    #[test]
    fn duplicate_mapping_is_rejected() {
        let mut tables = tables(16);
        assert!(tables.map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, WRITABLE));
        assert!(!tables.map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, WRITABLE));
    }

    #[test]
    fn noncanonical_address_is_rejected() {
        let mut tables = tables(8);
        assert!(!tables.map_kernel_range(0x0000_8000_0000_0000, 0x20_0000, PAGE_SIZE, WRITABLE));
        assert!(!tables.map_identity(0xFFFF_8000_0000_0000, PAGE_SIZE, WRITABLE));
    }

    #[test]
    fn out_of_frames_is_reported() {
        let mut tables = tables(2);
        assert!(!tables.map_kernel_range(KERNEL, 0x20_0000, PAGE_SIZE, WRITABLE));
    }

    #[test]
    fn two_roots_are_distinct() {
        let tables = tables(8);
        assert_ne!(tables.ttbr0(), tables.ttbr1());
    }
}
