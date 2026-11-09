// SPDX-License-Identifier: MIT
// Copyright (c) 2017 Jeremy Soller
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 four-level paging constants.

/// Shift from bytes to pages.
pub const PAGE_SHIFT: usize = 12;
/// Page size in bytes.
pub const PAGE_SIZE: u64 = 1 << PAGE_SHIFT;
/// Shift of a page-table index.
pub const PAGE_ENTRY_SHIFT: usize = 9;
/// Entries in one page table.
pub const PAGE_ENTRIES: usize = 1 << PAGE_ENTRY_SHIFT;
/// Number of paging levels (PML4, PDP, PD, PT).
pub const PAGE_LEVELS: usize = 4;
/// Physical-address bits stored in a PTE (bits 12..51).
pub const ENTRY_ADDRESS_WIDTH: usize = 40;
/// Usable physical-address mask (52 bits).
pub const PHYS_ADDR_MASK: u64 = (1 << 52) - 1;
/// The physical-address field of a PTE, shifted down to bit zero.
pub const ENTRY_ADDRESS_MASK: u64 = (1 << ENTRY_ADDRESS_WIDTH) - 1;
/// Mask of the flags in a PTE.
pub const ENTRY_FLAGS_MASK: u64 = !(ENTRY_ADDRESS_MASK << PAGE_SHIFT);

/// True for canonical four-level x86-64 addresses.
pub const fn is_canonical(address: u64) -> bool {
    let top = address >> 47;
    top == 0 || top == 0x1_ffff
}

/// Extract the physical address from a PTE.
pub const fn entry_address(entry: u64) -> u64 {
    ((entry >> PAGE_SHIFT) & ENTRY_ADDRESS_MASK) << PAGE_SHIFT
}

/// Extract the flag bits from a PTE.
pub const fn entry_flags(entry: u64) -> u64 {
    entry & ENTRY_FLAGS_MASK
}

const _: () = {
    assert!(PAGE_SIZE == 4096);
    assert!(PAGE_ENTRIES == 512);
    assert!(PHYS_ADDR_MASK == 0x000F_FFFF_FFFF_FFFF);
    assert!(ENTRY_ADDRESS_MASK == 0x0000_00FF_FFFF_FFFF);
    assert!(ENTRY_FLAGS_MASK == 0xFFF0_0000_0000_0FFF);
};

#[cfg(test)]
mod tests {
    use super::is_canonical;

    #[test]
    fn canonical_low_half() {
        assert!(is_canonical(0));
        assert!(is_canonical(0x0000_7FFF_FFFF_FFFF));
    }

    #[test]
    fn canonical_high_half() {
        assert!(is_canonical(0xFFFF_8000_0000_0000));
        assert!(is_canonical(0xFFFF_FFFF_FFFF_FFFF));
    }

    #[test]
    fn canonical_hole_is_rejected() {
        assert!(!is_canonical(0x0000_8000_0000_0000));
        assert!(!is_canonical(0x0000_FFFF_FFFF_FFFF));
        assert!(!is_canonical(0xFFFF_0000_0000_0000));
        assert!(!is_canonical(0xFFFF_7FFF_FFFF_FFFF));
    }
}
