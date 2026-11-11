// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure MSI/MSI-X message encoding.
//!
//! A driver programs the `(address_lo, address_hi, data)` triple returned by the kernel into the
//! device's MSI capability or MSI-X table entry.  The kernel compiles this crate by `#[path]`, so
//! the encoding cannot drift between the kernel and its host tests.

#![cfg_attr(not(target_os = "none"), no_std)]
#![forbid(unsafe_code)]
// The kernel includes this crate as a module and uses only part of its API.
#![cfg_attr(target_os = "none", allow(dead_code))]

/// Fixed delivery mode.
pub const DELIVERY_FIXED: u8 = 0;
/// Lowest-priority delivery mode.
pub const DELIVERY_LOWEST: u8 = 1;
/// SMI delivery mode.
pub const DELIVERY_SMI: u8 = 2;
/// NMI delivery mode.
pub const DELIVERY_NMI: u8 = 4;
/// INIT delivery mode.
pub const DELIVERY_INIT: u8 = 5;
/// ExtINT delivery mode.
pub const DELIVERY_EXTINT: u8 = 7;

/// The xAPIC MSI address page.
pub const MSI_ADDRESS_BASE: u32 = 0xFEE0_0000;

/// Encode the 64-bit MSI message address for `destination`.
///
/// The 8-bit destination APIC id goes in the low word's bits 19..12 and the upper word stays
/// zero.  This is the architectural message address in both xAPIC and x2APIC mode: the MSI
/// address format does not change with the local APIC mode.  The redirection hint and
/// destination-mode bits stay clear (physical destination).
///
/// Returns `None` for ids above 255.  The extended destination format is not implemented, and
/// the id must never be moved into the upper half: a caller that gets `None` has to fail with
/// `EOPNOTSUPP` (the BSD `ENOTSUP` value) instead of programming an unvalidated message.
pub const fn msi_address(destination: u32) -> Option<(u32, u32)> {
    if destination <= 0xFF {
        Some((MSI_ADDRESS_BASE | (destination << 12), 0))
    } else {
        None
    }
}

/// Encode the MSI message data word.
///
/// Bits 7..0 carry the vector, bits 10..8 the delivery mode, and bit 15 selects level triggering.
/// The assert bit is never set: the device asserts its interrupt line.
pub const fn msi_data(vector: u8, delivery_mode: u8, level: bool) -> u32 {
    (vector as u32) | (((delivery_mode & 0x7) as u32) << 8) | ((level as u32) << 15)
}

/// True when `vector` lies in a driver pool of `count` vectors starting at `base`.
pub const fn vector_in_pool(vector: u8, base: u8, count: u8) -> bool {
    vector >= base && vector.wrapping_sub(base) < count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_places_the_destination_in_bits_19_12() {
        assert_eq!(msi_address(0), Some((0xFEE0_0000, 0)));
        assert_eq!(msi_address(1), Some((0xFEE0_1000, 0)));
        assert_eq!(msi_address(0x7F), Some((0xFEE7_F000, 0)));
        assert_eq!(msi_address(0xFF), Some((0xFEEF_F000, 0)));
    }

    #[test]
    fn address_uses_the_same_encoding_in_x2apic_mode() {
        // x2APIC keeps the architectural message address: destination 0x10 is not moved into
        // the upper half.
        assert_eq!(msi_address(0x10), Some((0xFEE1_0000, 0)));
        assert_eq!(msi_address(0x10).map(|(_, high)| high), Some(0));
    }

    #[test]
    fn address_rejects_ids_above_255() {
        assert_eq!(msi_address(0x100), None);
        assert_eq!(msi_address(0xFFFF_FFEF), None);
        assert_eq!(msi_address(u32::MAX), None);
    }

    #[test]
    fn address_has_the_redirection_and_destination_bits_clear() {
        let Some((low, high)) = msi_address(0x3C) else {
            panic!("destination 0x3C must encode");
        };
        assert_eq!(low & 0x0000_000F, 0);
        assert_eq!(high, 0);
    }

    #[test]
    fn data_carries_the_vector_in_the_low_bits() {
        assert_eq!(msi_data(0x40, DELIVERY_FIXED, false), 0x40);
        assert_eq!(msi_data(0xBF, DELIVERY_FIXED, false), 0xBF);
    }

    #[test]
    fn data_carries_the_delivery_mode() {
        assert_eq!(msi_data(0x40, DELIVERY_LOWEST, false), 0x140);
        assert_eq!(msi_data(0x40, DELIVERY_SMI, false), 0x240);
        assert_eq!(msi_data(0x40, DELIVERY_NMI, false), 0x440);
        assert_eq!(msi_data(0x40, DELIVERY_INIT, false), 0x540);
        assert_eq!(msi_data(0x40, DELIVERY_EXTINT, false), 0x740);
    }

    #[test]
    fn data_masks_the_delivery_mode_to_three_bits() {
        assert_eq!(msi_data(0x40, 0x9, false), 0x140);
    }

    #[test]
    fn data_sets_the_level_bit() {
        assert_eq!(msi_data(0x40, DELIVERY_FIXED, true), 0x8040);
        assert_eq!(msi_data(0x40, DELIVERY_FIXED, false), 0x40);
    }

    #[test]
    fn data_never_sets_the_assert_bit() {
        for mode in 0..8 {
            assert_eq!(msi_data(0x40, mode, false) & (1 << 14), 0);
        }
    }

    #[test]
    fn pool_membership_is_exact() {
        assert!(vector_in_pool(0x40, 0x40, 128));
        assert!(vector_in_pool(0xBF, 0x40, 128));
        assert!(!vector_in_pool(0x3F, 0x40, 128));
        assert!(!vector_in_pool(0xC0, 0x40, 128));
        assert!(!vector_in_pool(0x20, 0x40, 128));
    }

    #[test]
    fn pool_membership_survives_wrapping() {
        assert!(vector_in_pool(0xFF, 0xF0, 16));
        assert!(!vector_in_pool(0xEF, 0xF0, 16));
    }

    #[test]
    fn empty_pool_has_no_members() {
        assert!(!vector_in_pool(0x40, 0x40, 0));
    }

    #[test]
    fn golden_msi_message_matches_the_architectural_layout() {
        // Destination 5, vector 0x42, fixed delivery, edge: the documented triple.
        let Some((lo, hi)) = msi_address(5) else {
            panic!("destination 5 must encode");
        };
        let data = msi_data(0x42, DELIVERY_FIXED, false);
        assert_eq!(lo, 0xFEE0_5000);
        assert_eq!(hi, 0);
        assert_eq!(data, 0x42);
    }

    #[test]
    fn destination_zero_is_the_boot_apic() {
        assert_eq!(msi_address(0), Some((MSI_ADDRESS_BASE, 0)));
    }
}
