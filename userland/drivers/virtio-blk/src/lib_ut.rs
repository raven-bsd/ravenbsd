// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the virtio-blk core: layout, descriptors, requests, pattern and marker.

use super::*;

#[test]
fn a_queue_layout_is_aligned_and_bounded() {
    let layout = RingLayout::new(0x40, 8);
    assert_eq!(layout.desc % 16, 0);
    assert_eq!(layout.avail % 2, 0);
    assert_eq!(layout.used % 4, 0);
    assert!(layout.desc < layout.avail && layout.avail < layout.used);
    assert_eq!(layout.end, layout.used + 4 + 8 * 8);
}

#[test]
fn queue_slots_cover_every_entry_and_nothing_else() {
    let layout = RingLayout::new(0, 4);
    assert_eq!(layout.avail_slot(0), Some(layout.avail + 4));
    assert_eq!(layout.avail_slot(3), Some(layout.avail + 10));
    assert_eq!(layout.avail_slot(4), None);
    assert_eq!(layout.used_slot(3), Some(layout.used + 4 + 24));
    assert_eq!(layout.used_slot(4), None);
    assert_eq!(layout.avail_idx(), layout.avail + 2);
    assert_eq!(layout.used_idx(), layout.used + 2);
}

#[test]
fn the_queue_size_is_a_bounded_power_of_two() {
    assert_eq!(queue_size(3), None);
    assert_eq!(queue_size(4), Some(4));
    assert_eq!(queue_size(7), Some(4));
    assert_eq!(queue_size(8), Some(8));
    assert_eq!(queue_size(256), Some(8));
}

#[test]
fn a_descriptor_and_a_used_entry_round_trip() {
    let descriptor = Descriptor {
        addr: 0x1234_5678,
        len: 512,
        flags: DESC_F_NEXT | DESC_F_WRITE,
        next: 2,
    };
    assert_eq!(
        decode_descriptor(&encode_descriptor(descriptor)),
        Some(descriptor)
    );
    assert_eq!(decode_descriptor(&[0u8; 15]), None);

    let mut used = [0u8; 8];
    used[0..4].copy_from_slice(&0u32.to_le_bytes());
    used[4..8].copy_from_slice(&512u32.to_le_bytes());
    assert_eq!(decode_used(&used), Some(UsedEntry { id: 0, len: 512 }));
    assert_eq!(decode_used(&[0u8; 7]), None);
}

#[test]
fn a_request_header_round_trips() {
    let bytes = encode_request(BLK_T_OUT, 0x1_0000_0002);
    assert_eq!(decode_request(&bytes), Some((BLK_T_OUT, 0x1_0000_0002)));
    assert_eq!(decode_request(&bytes[..15]), None);

    let mut reserved = bytes;
    reserved[4] = 1;
    assert_eq!(decode_request(&reserved), None);
}

#[test]
fn a_valid_read_is_accepted() {
    assert_eq!(validate_request(BLK_T_IN, 0, 1, 100, false), Ok(512));
    assert_eq!(validate_request(BLK_T_IN, 99, 1, 100, false), Ok(512));
    assert_eq!(
        validate_request(BLK_T_IN, 0, MAX_SECTORS, 1000, false),
        Ok((MAX_SECTORS * SECTOR_SIZE) as usize)
    );
    assert_eq!(validate_request(BLK_T_OUT, 5, 2, 100, false), Ok(1024));
}

#[test]
fn a_request_outside_the_device_is_refused() {
    assert_eq!(
        validate_request(BLK_T_IN, 100, 1, 100, false),
        Err(RequestError::BadRange)
    );
    assert_eq!(
        validate_request(BLK_T_IN, u64::MAX, 2, 100, false),
        Err(RequestError::BadRange)
    );
    assert_eq!(
        validate_request(BLK_T_IN, 0, 0, 100, false),
        Err(RequestError::BadRange)
    );
    assert_eq!(
        validate_request(BLK_T_IN, 0, MAX_SECTORS + 1, 1000, false),
        Err(RequestError::BadRange)
    );
}

#[test]
fn a_write_on_a_read_only_device_is_refused() {
    assert_eq!(
        validate_request(BLK_T_OUT, 0, 1, 100, true),
        Err(RequestError::ReadOnly)
    );
    assert_eq!(validate_request(BLK_T_IN, 0, 1, 100, true), Ok(512));
}

#[test]
fn flush_takes_no_sector_and_no_count() {
    assert_eq!(validate_request(BLK_T_FLUSH, 0, 0, 100, false), Ok(0));
    assert_eq!(
        validate_request(BLK_T_FLUSH, 1, 0, 100, false),
        Err(RequestError::BadRange)
    );
    assert_eq!(
        validate_request(BLK_T_FLUSH, 0, 1, 100, false),
        Err(RequestError::BadRange)
    );
    assert_eq!(
        validate_request(99, 0, 0, 100, false),
        Err(RequestError::BadOp)
    );
}

#[test]
fn the_pattern_round_trips_and_pins_the_lba() {
    let mut bytes = [0u8; 3 * SECTOR_SIZE as usize];
    fill_pattern(40, 0x3c, &mut bytes);
    assert!(verify_pattern(40, 0x3c, &bytes));
    assert!(!verify_pattern(41, 0x3c, &bytes));
    assert!(!verify_pattern(40, 0x3d, &bytes));
    assert!(!verify_pattern(40, 0x3c, &bytes[..511]));

    let mut single = [0u8; SECTOR_SIZE as usize];
    fill_pattern(0x1_0000_0000, 0xa5, &mut single);
    assert_eq!(single.get(..8), Some(&0x1_0000_0000u64.to_le_bytes()[..]));
    assert!(
        single
            .get(8..)
            .is_some_and(|rest| rest.iter().all(|b| *b == 0xa5))
    );
}

#[test]
fn the_hasher_is_deterministic_and_sensitive() {
    let mut bytes = [0u8; 2 * SECTOR_SIZE as usize];
    fill_pattern(7, 0xa5, &mut bytes);
    let mut first = Hasher::new();
    first.update(&bytes);
    let mut second = Hasher::new();
    second.update(&bytes);
    assert_eq!(first.finish(), second.finish());

    let mut changed = bytes;
    changed[100] ^= 0xff;
    let mut third = Hasher::new();
    third.update(&changed);
    assert_ne!(first.finish(), third.finish());
}

#[test]
fn the_marker_round_trips() {
    let sector = marker_encode(0x1234_5678_9abc_def0);
    assert_eq!(marker_decode(&sector), Some(0x1234_5678_9abc_def0));

    let mut bad = sector;
    bad[0] = b'X';
    assert_eq!(marker_decode(&bad), None);
    assert_eq!(marker_decode(&sector[..511]), None);
}

#[test]
fn feature_words_are_queried_by_absolute_bit() {
    let features = Features {
        low: BLK_F_RO | BLK_F_FLUSH,
        high: F_VERSION_1,
    };
    assert!(features.has(5));
    assert!(features.has(9));
    assert!(features.has(32));
    assert!(!features.has(0));
    assert!(!features.has(64));
}

#[test]
fn constants_match_the_protocol() {
    assert_eq!(SECTOR_SIZE, 512);
    assert_eq!(MAX_SECTORS, 128);
    assert_eq!(CHUNK_SECTORS * SECTOR_SIZE, 65_024);
    assert_eq!(BLK_S_OK, 0);
    assert_eq!(BLK_T_IN, 0);
}
