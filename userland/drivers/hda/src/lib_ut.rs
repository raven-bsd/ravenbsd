// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the HDA byte-level core.

use super::*;

#[test]
fn commands_follow_the_4_8_12_8_layout() {
    assert_eq!(cmd12(0, 0x04, 0xF00, 0x05), 0x004F_0005);
    assert_eq!(cmd12(0, 0x02, 0x706, 0x10), 0x0027_0610);
    assert_eq!(cmd4(0, 0x02, 0x0002, 0x1234), 0x0022_1234);
}

#[test]
fn the_48k_format_word_is_stereo_s16le() {
    assert_eq!(format_48k(2), 0x11);
    assert_eq!(format_48k(1), 0x10);
    assert_eq!(format_48k(8), 0x17);
}

#[test]
fn the_bdl_entry_is_sixteen_bytes() {
    let mut buffer = [0u8; 16];
    let entry = BdlEntry {
        address: 0x1234_5678_9abc_def0,
        length: 0x200,
        ioc: true,
    };
    assert!(entry.write(&mut buffer, 0));
    assert_eq!(&buffer[0..8], &0x1234_5678_9abc_def0u64.to_le_bytes());
    assert_eq!(&buffer[8..12], &0x200u32.to_le_bytes());
    assert_eq!(buffer[12], 1);
    assert!(!entry.write(&mut buffer, 1));
}

#[test]
fn ring_geometry_matches_the_shared_header_rule() {
    let geometry = RingGeometry {
        period_frames: 480,
        period_count: 4,
        frame_bytes: 4,
    };
    assert_eq!(geometry.region_len(), 128 + 480 * 4 * 4);
    assert_eq!(geometry.cbl_bytes(), 480 * 4 * 4);
}

#[test]
fn register_offsets_are_the_ich9_values() {
    assert_eq!(GCTL, 0x08);
    assert_eq!(CORBLBASE, 0x40);
    assert_eq!(RIRBWP, 0x58);
    assert_eq!(ICOI, 0x60);
    assert_eq!(SD_BASE, 0x80);
    assert_eq!(SD_STRIDE, 0x20);
    assert_eq!(SD_BDPL, 0x18);
}
