// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn utf8_decode_hostile_ok() {
    body(b"\x01\x10\xc0\x80\xed\xa0\x80\xf4\x90\x80\x80\x80");
}

#[test]
fn utf8_decode_boundaries_ok() {
    let mut input = vec![1u8, 4];
    input.extend_from_slice(&[0xc0, 0x80, 0xe0, 0x80, 0x80]);
    input.extend_from_slice(&[
        0xed, 0xa0, 0x80, 0xf4, 0x90, 0x80, 0x80, 0xf8, 0x88, 0x80, 0x80, 0x80,
    ]);
    input.push(0x80);
    body(&input);
    body(&[0u8, 1, b'a', 0x80, 0xff]);
    body(&[1u8, 16]);
    body(&[0u8, 1, 0, 0, 0, 0]);
}

#[test]
#[should_panic(expected = "P-utf8-2")]
fn utf8_decode_mutation_2_fires() {
    let (left, _, _) = decode(b"abc", MbMode::Utf8, 1);
    let (right, _, _) = decode(b"abc", MbMode::Utf8, 2);
    let mut broken = left;
    if let Some(item) = broken.first_mut() {
        item.wc = 99;
    }
    assert!(
        broken
            .iter()
            .map(|item| item.wc)
            .eq(right.iter().map(|item| item.wc)),
        "P-utf8-2: the decoded characters change with a 2-byte split"
    );
}

#[test]
fn split_recovery_stops_at_the_same_error() {
    body(&[1, 16, 0xf1, 0xda, 0xbd, 0x40]);
    body(&[1, 16, 0xf0, 0x9f, 0x98, 0x80]);
    body(&[1, 16, 0xe2, 0x82, 0xac, 0x80]);
}

#[test]
fn every_split_count_runs() {
    for split in [0u8, 1, 2, 3, 4, 8, 16, 255] {
        body(&[1, split, 0xf0, 0x9f, 0x98, 0x80]);
    }
}

#[test]
fn byte_mode_maps_high_bytes() {
    let (decoded, invalid, incomplete) = decode(&[0x80, 0xff, b'a'], MbMode::Byte, 1);
    assert!(!invalid && !incomplete);
    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[0].wc, 0xdf80);
    assert_eq!(decoded[1].wc, 0xdfff);
}
