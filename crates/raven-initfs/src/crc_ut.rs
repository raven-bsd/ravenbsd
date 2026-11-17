// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::{POLYNOMIAL, crc32};

/// The original table-free loop, kept as the behavioural reference.
fn reference(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (POLYNOMIAL & mask);
        }
    }
    !crc
}

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

#[test]
fn slicing_matches_the_bitwise_reference_for_every_tail_length() {
    let mut state = 0x5EED_1234_ABCD_9876u64;
    for length in 0..=259usize {
        let mut data = vec![0u8; length];
        for byte in &mut data {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        assert_eq!(crc32(&data), reference(&data), "length {length}");
    }
}
