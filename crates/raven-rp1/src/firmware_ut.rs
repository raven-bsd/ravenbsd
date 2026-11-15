// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the firmware record.

use super::*;

/// A synthetic digest: the test values are a byte ramp, not an RP1 firmware hash.
const GOOD: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

#[test]
fn a_digest_round_trips_its_hex_form() {
    let digest = Digest::from_hex(GOOD).expect("digest");
    assert_eq!(digest.to_string(), GOOD);
    assert_eq!(digest.as_bytes()[0], 0x00);
    assert_eq!(digest.as_bytes()[31], 0x1F);
}

#[test]
fn uppercase_hex_is_accepted() {
    let upper = "000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F";
    assert_eq!(
        Digest::from_hex(upper).expect("digest"),
        Digest::from_hex(GOOD).expect("digest")
    );
}

#[test]
fn a_wrong_length_is_rejected() {
    assert_eq!(Digest::from_hex("00"), Err(DigestError::Length(2)));
    assert_eq!(Digest::from_hex(""), Err(DigestError::Length(0)));
}

#[test]
fn a_non_hexadecimal_character_is_rejected() {
    let mut text = GOOD.to_string();
    text.replace_range(10..11, "g");
    assert_eq!(
        Digest::from_hex(&text),
        Err(DigestError::Digit {
            index: 10,
            byte: b'g'
        })
    );
}

#[test]
fn an_unrecorded_image_is_not_usable() {
    assert!(!FirmwareImage::UNRECORDED.is_recorded());
}

#[test]
fn a_complete_record_is_usable() {
    let digest = Digest::from_hex(GOOD).expect("digest");
    let image = FirmwareImage::recorded("synthetic-test-image.bin", digest, 0x40_0000, "owner");
    assert!(image.is_recorded());
    assert_eq!(image.size, Some(0x40_0000));
}

#[test]
fn unknown_markers_keep_a_partial_record_unusable() {
    let digest = Digest::from_hex(GOOD).expect("digest");
    let image = FirmwareImage {
        digest: Some(digest),
        size: Some(0x40_0000),
        ..FirmwareImage::UNRECORDED
    };
    assert!(!image.is_recorded());
}
