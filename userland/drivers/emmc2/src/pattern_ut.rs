// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the deterministic pattern, checksum and marker.

use super::*;

#[test]
fn the_pattern_stamps_every_sector_and_fills_the_rest() {
    let mut bytes = [0u8; 4 * SECTOR_SIZE];
    fill_pattern(0x100, 0xa5, &mut bytes);
    assert!(verify_pattern(0x100, 0xa5, &bytes));
    assert_eq!(&bytes[..8], &0x100u64.to_le_bytes());
    assert_eq!(
        &bytes[SECTOR_SIZE..SECTOR_SIZE + 8],
        &0x101u64.to_le_bytes()
    );
    assert!(bytes[8..SECTOR_SIZE].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn a_wrong_sector_or_fill_is_detected() {
    let mut bytes = [0u8; 2 * SECTOR_SIZE];
    fill_pattern(7, 0x3c, &mut bytes);
    assert!(!verify_pattern(8, 0x3c, &bytes));
    assert!(!verify_pattern(7, 0x3d, &bytes));
    bytes[SECTOR_SIZE + 16] ^= 1;
    assert!(!verify_pattern(7, 0x3c, &bytes));
    assert!(!verify_pattern(7, 0x3c, &bytes[..SECTOR_SIZE - 1]));
}

#[test]
fn the_hash_is_deterministic_and_word_sensitive() {
    let empty = Hasher::new().finish();
    assert_eq!(empty, 0xcbf2_9ce4_8422_2325);

    let mut left = Hasher::new();
    left.update(b"RavenBSD");
    let mut right = Hasher::new();
    right.update(b"RavenBSD");
    assert_eq!(left.finish(), right.finish());

    let mut other = Hasher::new();
    other.update(b"RavenBSd");
    assert_ne!(left.finish(), other.finish());

    let mut tailed = Hasher::new();
    tailed.update(b"RavenBSD!");
    assert_ne!(left.finish(), tailed.finish());
}

#[test]
fn the_hash_folds_words_in_order() {
    let mut first = Hasher::new();
    first.update(&[1, 0, 0, 0, 0, 0, 0, 0]);
    first.update(&[2, 0, 0, 0, 0, 0, 0, 0]);

    let mut second = Hasher::new();
    second.update(&[2, 0, 0, 0, 0, 0, 0, 0]);
    second.update(&[1, 0, 0, 0, 0, 0, 0, 0]);
    assert_ne!(first.finish(), second.finish());
}

#[test]
fn the_hash_is_chunking_independent() {
    let bytes = [0x5au8; 4 * SECTOR_SIZE];
    let mut whole = Hasher::new();
    whole.update(&bytes);

    let mut chunked = Hasher::new();
    for chunk in bytes.chunks(SECTOR_SIZE) {
        chunked.update(chunk);
    }
    assert_eq!(whole.finish(), chunked.finish());
}

#[test]
fn the_marker_round_trips() {
    let marker = marker_encode(0x1234_5678_9abc_def0);
    assert_eq!(marker_decode(&marker), Some(0x1234_5678_9abc_def0));
    assert_eq!(&marker[..16], MARKER_MAGIC);
}

#[test]
fn a_bad_or_short_marker_is_rejected() {
    assert_eq!(marker_decode(&[0u8; 512]), None);
    assert_eq!(marker_decode(&[0u8; 32]), None);
    let mut marker = marker_encode(9);
    marker[0] ^= 1;
    assert_eq!(marker_decode(&marker), None);
}
