// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the panel geometry and row packing.

use super::*;

#[test]
fn default_profile_is_800x480() {
    assert_eq!(DEFAULT_WIDTH, 800);
    assert_eq!(DEFAULT_HEIGHT, 480);
    assert_eq!(DEFAULT_ROW_BYTES, 100);
    assert_eq!(DEFAULT_FRAME_BYTES, 48_000);
    assert_eq!(Waveshare75V2::WIDTH, DEFAULT_WIDTH);
    assert_eq!(Waveshare75V2::HEIGHT, DEFAULT_HEIGHT);
    assert_eq!(Waveshare75V2::ROW_BYTES, DEFAULT_ROW_BYTES);
}

#[test]
fn waveshare_packing_inverts_black_to_the_panel_ram() {
    let row = [0x00, 0xff, 0xaa, 0x55];
    let mut out = [0u8; DEFAULT_ROW_BYTES];
    let mut padded = [0u8; DEFAULT_ROW_BYTES];
    padded[..row.len()].copy_from_slice(&row);
    Waveshare75V2::pack_row(&padded, &mut out).unwrap();
    assert_eq!(&out[..4], [0xff, 0x00, 0x55, 0xaa]);
}

#[test]
fn packing_rejects_short_buffers() {
    let mut out = [0u8; 4];
    assert_eq!(
        Waveshare75V2::pack_row(&[0u8; 8], &mut out[..4]),
        Err(PackError::BufferTooSmall)
    );
    let mut short = [0u8; 4];
    assert_eq!(
        Waveshare75V2::pack_row(&[0u8; DEFAULT_ROW_BYTES], &mut short[..4]),
        Err(PackError::BufferTooSmall)
    );
}
