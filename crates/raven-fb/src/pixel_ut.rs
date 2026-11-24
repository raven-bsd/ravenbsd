// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pixel formats.

use super::*;

#[test]
fn depth_maps_to_the_supported_formats() {
    assert_eq!(PixelFormat::from_depth(32), Some(PixelFormat::Rgb888));
    assert_eq!(PixelFormat::from_depth(16), Some(PixelFormat::Rgb565));
    assert_eq!(PixelFormat::from_depth(8), Some(PixelFormat::Indexed8));
    assert_eq!(PixelFormat::from_depth(24), None);
    assert_eq!(PixelFormat::from_depth(1), None);
}

#[test]
fn bytes_per_pixel_matches_the_depth() {
    assert_eq!(PixelFormat::Rgb888.bytes_per_pixel(), 4);
    assert_eq!(PixelFormat::Rgb565.bytes_per_pixel(), 2);
    assert_eq!(PixelFormat::Indexed8.bytes_per_pixel(), 1);
}

#[test]
fn black_is_zero_and_white_is_the_full_colour() {
    let cases = [
        (PixelFormat::Rgb888, 0x00ff_ffff),
        (PixelFormat::Rgb565, 0xffff),
        (PixelFormat::Indexed8, 0xff),
    ];
    for (format, expected) in cases {
        assert_eq!(format.encode(Color::Black), 0, "{format:?}");
        assert_eq!(format.encode(Color::White), expected, "{format:?}");
    }
}
