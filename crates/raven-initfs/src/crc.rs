// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CRC-32 (IEEE 802.3, reflected) used to validate every file in the archive.

/// Compute the IEEE 802.3 CRC-32 of `data`.
///
/// Reflected polynomial `0xEDB8_8320`, initial value `0xFFFF_FFFF`, final XOR with
/// `0xFFFF_FFFF`. The implementation is table-free so the reader stays small and allocation-free.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::crc32;

    #[test]
    fn check_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn empty_input_is_zero() {
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn single_bit_flip_changes_the_value() {
        let original = crc32(b"ravenbsd");
        for byte_index in 0..8usize {
            for bit in 0..8u32 {
                let mut data = *b"ravenbsd";
                data[byte_index] ^= 1 << bit;
                assert_ne!(crc32(&data), original, "byte {byte_index} bit {bit}");
            }
        }
    }
}
