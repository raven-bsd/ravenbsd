// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the 1-bit framebuffer, rotation, clipping and packing.

use super::*;
use crate::profile::{DEFAULT_FRAME_BYTES, PackError, PanelProfile};

fn count(fb: &Framebuffer) -> usize {
    let mut count = 0usize;
    let mut y = 0usize;
    while y < fb.panel_height() {
        let mut x = 0usize;
        while x < fb.panel_width() {
            if fb.physical_pixel(x, y) {
                count += 1;
            }
            x += 1;
        }
        y += 1;
    }
    count
}

#[test]
fn default_framebuffer_is_white_800x480() {
    let fb = Framebuffer::new();
    assert_eq!(fb.panel_width(), 800);
    assert_eq!(fb.panel_height(), 480);
    assert_eq!(fb.width(), 800);
    assert_eq!(fb.height(), 480);
    assert_eq!(fb.rotation(), Rotation::None);
    assert!(!fb.pixel(0, 0));
    assert!(!fb.pixel(799, 479));
    assert_eq!(count(&fb), 0);
}

#[test]
fn clear_and_pixel_roundtrip() {
    let mut fb = Framebuffer::new();
    fb.clear(true);
    assert!(fb.pixel(0, 0));
    assert!(fb.pixel(799, 479));
    assert_eq!(count(&fb), 800 * 480);
    fb.set_pixel(5, 5, false);
    assert!(!fb.pixel(5, 5));
    assert!(!fb.set_pixel(-1, 0, true));
    assert!(!fb.set_pixel(800, 0, true));
    assert!(!fb.set_pixel(0, 480, true));
}

#[test]
fn rotation_maps_logical_to_physical_corners() {
    let mut fb = Framebuffer::new();
    fb.set_rotation(Rotation::Quarter);
    assert_eq!((fb.width(), fb.height()), (480, 800));
    assert!(fb.set_pixel(0, 0, true));
    assert!(fb.physical_pixel(799, 0));
    assert!(fb.set_pixel(479, 799, true));
    assert!(fb.physical_pixel(0, 479));
    assert!(!fb.set_pixel(480, 0, true));

    fb.clear(false);
    fb.set_rotation(Rotation::Half);
    assert!(fb.set_pixel(0, 0, true));
    assert!(fb.physical_pixel(799, 479));

    fb.clear(false);
    fb.set_rotation(Rotation::ThreeQuarter);
    assert_eq!((fb.width(), fb.height()), (480, 800));
    assert!(fb.set_pixel(0, 0, true));
    assert!(fb.physical_pixel(0, 479));
    assert!(fb.set_pixel(479, 799, true));
    assert!(fb.physical_pixel(799, 0));
}

#[test]
fn line_and_rect_clip_at_the_panel_edge() {
    let mut fb = Framebuffer::new();
    fb.line(-2, 2, 2, 2, true);
    assert!(fb.pixel(0, 2));
    assert!(fb.pixel(1, 2));
    assert!(fb.pixel(2, 2));
    assert_eq!(count(&fb), 3);

    fb.clear(false);
    fb.rect(-2, -2, 4, 4, true);
    assert!(fb.pixel(0, 0));
    assert!(fb.pixel(1, 1));
    assert_eq!(count(&fb), 4);

    fb.clear(false);
    fb.rect(798, 478, 4, 4, true);
    assert!(fb.pixel(799, 479));
    assert_eq!(count(&fb), 4);
}

#[test]
fn known_bitmap_and_glyph_blit() {
    let data = [0b1000_0001, 0b0100_0010];
    let glyph = Glyph {
        width: 8,
        height: 2,
        data: &data,
    };
    let mut fb = Framebuffer::new();
    assert_eq!(fb.blit_glyph(0, 0, &glyph, true), 4);
    assert!(fb.pixel(0, 0));
    assert!(fb.pixel(7, 0));
    assert!(fb.pixel(1, 1));
    assert!(fb.pixel(6, 1));
    assert_eq!(count(&fb), 4);

    fb.clear(false);
    assert_eq!(fb.blit_bitmap(796, 0, 8, 2, &data, true), 2);
    assert!(fb.pixel(796, 0));
    assert!(fb.pixel(797, 1));
    assert_eq!(count(&fb), 2);

    fb.clear(false);
    assert_eq!(fb.blit_bitmap(0, 0, 8, 2, &[], true), 0);
}

#[test]
fn pack_uses_black_to_white_panel_convention() {
    let mut fb = Framebuffer::new();
    let mut packed = [0u8; DEFAULT_FRAME_BYTES];
    assert_eq!(fb.pack_default(&mut packed).unwrap(), DEFAULT_FRAME_BYTES);
    assert_eq!(packed[0], 0xff);
    assert_eq!(packed[DEFAULT_FRAME_BYTES - 1], 0xff);

    fb.clear(true);
    fb.pack_default(&mut packed).unwrap();
    assert_eq!(packed[0], 0x00);
    assert_eq!(packed[DEFAULT_FRAME_BYTES - 1], 0x00);

    fb.clear(false);
    fb.set_pixel(0, 0, true);
    fb.pack_default(&mut packed).unwrap();
    assert_eq!(packed[0], 0x7f);
    assert_eq!(packed[1], 0xff);
}

#[test]
fn pack_rejects_short_output_and_wrong_geometry() {
    struct Wrong;
    impl PanelProfile for Wrong {
        const WIDTH: usize = 799;
        const HEIGHT: usize = 480;
        const ROW_BYTES: usize = 100;
        fn pack_row(_: &[u8], _: &mut [u8]) -> Result<(), PackError> {
            Ok(())
        }
    }

    let fb = Framebuffer::new();
    let mut short = [0u8; 4];
    assert_eq!(fb.pack_default(&mut short), Err(PackError::BufferTooSmall));
    let mut packed = [0u8; DEFAULT_FRAME_BYTES];
    assert_eq!(
        fb.pack::<Wrong>(&mut packed),
        Err(PackError::DimensionMismatch)
    );
}
