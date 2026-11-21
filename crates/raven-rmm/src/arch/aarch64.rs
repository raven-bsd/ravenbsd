// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 translation-table constants (4 KiB granule, 48-bit VA).
//!
//! ARMv8-A facts applied here (matching the S12 loader's TTBR1 encoder and the NetBSD pmap
//! design in `sys/arch/aarch64/aarch64/pmap.c`):
//!
//! * 4 KiB granule, 4 levels (L0..L3), 512 entries per table, 48-bit virtual addresses with
//!   `TCR_EL1.T0SZ = T1SZ = 16`.
//! * Descriptors: valid bit 0, table/page bit 1, `AttrIndx[4:2]`, `AP[7:6]`
//!   (`0b00` EL1 RW, `0b10` EL1 RO, `0b01` EL0+EL1 RW, `0b11` EL0+EL1 RO), `SH[9:8]` inner
//!   shareable `0b11`, `AF` bit 10, `nG` bit 11, `PXN` bit 53, `UXN` bit 54. Table descriptors
//!   leave `PXNTable`/`UXNTable` (bits 59/60) clear.
//! * `MAIR_EL1`: attr0 `0x00` device-nGnRnE, attr1 `0xFF` normal write-back, attr2 `0x44`
//!   normal non-cacheable, attr3 `0x04` device-nGnRE.
//!
//! Execution is controlled per privilege level (PXN/UXN) and address validity distinguishes
//! the TTBR0 and TTBR1 halves.

/// Shift from bytes to pages.
pub const PAGE_SHIFT: usize = 12;
/// Page size in bytes.
pub const PAGE_SIZE: u64 = 1 << PAGE_SHIFT;
/// Shift of a page-table index.
pub const PAGE_ENTRY_SHIFT: usize = 9;
/// Entries in one page table.
pub const PAGE_ENTRIES: usize = 1 << PAGE_ENTRY_SHIFT;
/// Number of paging levels (L0, L1, L2, L3).
pub const PAGE_LEVELS: usize = 4;
/// Physical-address bits stored in a descriptor (bits 12..47 for the 48-bit format).
pub const ENTRY_ADDRESS_WIDTH: usize = 36;
/// Usable physical-address mask (48 bits).
pub const PHYS_ADDR_MASK: u64 = (1 << 48) - 1;
/// The physical-address field of a descriptor, shifted down to bit zero.
pub const ENTRY_ADDRESS_MASK: u64 = (1 << ENTRY_ADDRESS_WIDTH) - 1;
/// Mask of the attribute/permission bits in a descriptor.
pub const ENTRY_FLAGS_MASK: u64 = 0xFFFF_FFFF_F000_0FFF;

/// Descriptor is valid.
pub const ENTRY_VALID: u64 = 1 << 0;
/// Table descriptor or L3 page descriptor.
pub const ENTRY_TABLE_OR_PAGE: u64 = 1 << 1;
/// Attribute index in `MAIR_EL1`: attr0 device-nGnRnE.
pub const ATTR_DEVICE: u64 = 0b000 << 2;
/// Attribute index in `MAIR_EL1`: attr1 normal write-back.
pub const ATTR_NORMAL: u64 = 0b001 << 2;
/// Attribute index in `MAIR_EL1`: attr2 normal non-cacheable.
pub const ATTR_NON_CACHEABLE: u64 = 0b010 << 2;
/// Attribute index in `MAIR_EL1`: attr3 device-nGnRE.
pub const ATTR_DEVICE_NGNRE: u64 = 0b011 << 2;
/// Inner-shareable shareability.
pub const SH_INNER: u64 = 0b11 << 8;
/// Access flag.
pub const AF: u64 = 1 << 10;
/// Not global.
pub const NG: u64 = 1 << 11;
/// Privileged execute-never.
pub const PXN: u64 = 1 << 53;
/// User execute-never.
pub const UXN: u64 = 1 << 54;
/// AP `0b00`: EL1 read/write.
pub const AP_EL1_RW: u64 = 0b00 << 6;
/// AP `0b01`: EL0+EL1 read/write.
pub const AP_ALL_RW: u64 = 0b01 << 6;
/// AP `0b10`: EL1 read-only.
pub const AP_EL1_RO: u64 = 0b10 << 6;
/// AP `0b11`: EL0+EL1 read-only.
pub const AP_ALL_RO: u64 = 0b11 << 6;

/// `MAIR_EL1` value: attr0 `0x00` device-nGnRnE, attr1 `0xFF` normal write-back, attr2 `0x44`
/// normal non-cacheable, attr3 `0x04` device-nGnRE.
///
/// The loader's value is `0xFF00` (attr0/attr1 only); the kernel uses this extended value.
pub const MAIR_EL1: u64 = 0x0444_FF00;

/// `TCR_EL1` with 4 KiB granules, `T0SZ = T1SZ = 16` and the given `ID_AA64MMFR0_EL1.PARange`
/// IPS value; identical to the loader's `tcr()`.
pub const fn tcr_el1(physical_bits: u32) -> u64 {
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
        | (0b01 << 10) // ORGN0
        | (0b11 << 12) // SH0: inner shareable; TG0 = 0b00 (4 KiB) is implicit
        | (0b01 << 24) // IRGN1
        | (0b01 << 26) // ORGN1
        | (0b11 << 28) // SH1: inner shareable
        | (0b10 << 30) // TG1: 4 KiB
        | (ips << 32)
}

/// True for canonical aarch64 addresses.
///
/// Both halves use 48-bit VAs: TTBR0 covers `0x0000..`, TTBR1 covers `0xFFFF..`.
pub const fn is_canonical(address: u64) -> bool {
    let top = address >> 48;
    top == 0 || top == 0xFFFF
}

/// True for addresses translated by TTBR0 (the user half).
pub const fn is_user_half(address: u64) -> bool {
    (address >> 48) == 0
}

/// True for addresses translated by TTBR1 (the kernel half).
pub const fn is_kernel_half(address: u64) -> bool {
    (address >> 48) == 0xFFFF
}

/// Extract the physical address from a descriptor.
pub const fn entry_address(entry: u64) -> u64 {
    ((entry >> PAGE_SHIFT) & ENTRY_ADDRESS_MASK) << PAGE_SHIFT
}

/// Extract the attribute/permission bits from a descriptor.
pub const fn entry_flags(entry: u64) -> u64 {
    entry & ENTRY_FLAGS_MASK
}

const _: () = {
    assert!(PAGE_SIZE == 4096);
    assert!(PAGE_ENTRIES == 512);
    assert!(PHYS_ADDR_MASK == 0x0000_FFFF_FFFF_FFFF);
    assert!(ENTRY_ADDRESS_MASK == 0x0000_000F_FFFF_FFFF);
    assert!(MAIR_EL1 & 0xff == 0x00);
    assert!((MAIR_EL1 >> 8) & 0xff == 0xff);
    assert!((MAIR_EL1 >> 16) & 0xff == 0x44);
    assert!((MAIR_EL1 >> 24) & 0xff == 0x04);
};

#[cfg(test)]
#[path = "aarch64_ut.rs"]
mod tests;
