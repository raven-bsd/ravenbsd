// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The wide-word checksum must equal the textbook 16-bit loop for every length, split and
//! content, including odd slices that continue a word into the next slice.

use raven_tcpip_wire::checksum::internet_checksum;

fn reference(parts: &[&[u8]]) -> u16 {
    let stream: Vec<u8> = parts.iter().flat_map(|part| part.iter().copied()).collect();
    let mut sum: u32 = 0;
    for pair in stream.chunks(2) {
        let word = u16::from_be_bytes([pair[0], pair.get(1).copied().unwrap_or(0)]);
        sum += u32::from(word);
        while sum > 0xffff {
            sum = (sum & 0xffff) + (sum >> 16);
        }
    }
    !(sum as u16)
}

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn every_length_up_to_two_hundred_matches_the_reference() {
    let mut rng = Xorshift(0x1234_5678_9abc_def1);
    for length in 0..200usize {
        let data: Vec<u8> = (0..length).map(|_| rng.next() as u8).collect();
        assert_eq!(
            internet_checksum(&[&data]),
            reference(&[&data]),
            "length {length}"
        );
    }
}

#[test]
fn all_ones_and_all_zeros_carry_correctly() {
    for length in [0usize, 1, 2, 7, 8, 9, 15, 16, 1500, 1501, 65_535] {
        for fill in [0x00u8, 0xff, 0x80, 0x01] {
            let data = vec![fill; length];
            assert_eq!(
                internet_checksum(&[&data]),
                reference(&[&data]),
                "length {length} fill {fill:#x}"
            );
        }
    }
}

#[test]
fn arbitrary_splits_including_odd_boundaries_match_the_reference() {
    let mut rng = Xorshift(0xfeed_f00d_dead_beef);
    for _ in 0..2_000 {
        let length = (rng.next() % 3_000) as usize;
        let data: Vec<u8> = (0..length).map(|_| rng.next() as u8).collect();
        let cuts = (rng.next() % 4) as usize;
        let mut points: Vec<usize> = (0..cuts)
            .map(|_| (rng.next() as usize) % (length + 1))
            .collect();
        points.sort_unstable();
        let mut parts: Vec<&[u8]> = Vec::new();
        let mut start = 0;
        for point in points {
            parts.push(&data[start..point]);
            start = point;
        }
        parts.push(&data[start..]);
        assert_eq!(
            internet_checksum(&parts),
            reference(&parts),
            "length {length}"
        );
    }
}

#[test]
fn a_valid_packet_verifies_to_zero() {
    let mut packet: Vec<u8> = (0..1_001u32).map(|index| (index * 7) as u8).collect();
    packet[10] = 0;
    packet[11] = 0;
    let sum = internet_checksum(&[&packet]);
    packet[10..12].copy_from_slice(&sum.to_be_bytes());
    assert_eq!(internet_checksum(&[&packet]), 0);
}
