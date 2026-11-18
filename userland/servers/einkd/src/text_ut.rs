// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for ASCII text layout.

use std::string::String;

use raven_eink::{Framebuffer, Glyph};

use super::*;
use crate::content::Font;

static BLOCK: [u8; 1] = [0x80];

struct BlockFont;

impl Font for BlockFont {
    fn glyph(&self, _: u8) -> Option<Glyph<'_>> {
        Some(Glyph {
            width: 1,
            height: 1,
            data: &BLOCK,
        })
    }
}

fn style(align: Align, scale: u8, margin: i32, line_gap: i32) -> TextStyle {
    TextStyle {
        align,
        scale,
        margin,
        line_gap,
    }
}

#[test]
fn empty_text_clears_to_white() {
    let mut frame = Framebuffer::new();
    frame.clear(true);
    let layout = render_text(&mut frame, "", &BlockFont, style(Align::Left, 1, 0, 0)).unwrap();
    assert_eq!(layout, Layout::default());
    assert!(!frame.pixel(0, 0));
}

#[test]
fn alignment_places_each_visual_line() {
    let mut frame = Framebuffer::new();
    render_text(&mut frame, "AB", &BlockFont, style(Align::Left, 1, 0, 0)).unwrap();
    assert!(frame.pixel(0, 0));
    assert!(frame.pixel(1, 0));

    frame.clear(false);
    render_text(&mut frame, "AB", &BlockFont, style(Align::Center, 1, 0, 0)).unwrap();
    assert!(frame.pixel(399, 0));
    assert!(frame.pixel(400, 0));

    frame.clear(false);
    render_text(&mut frame, "AB", &BlockFont, style(Align::Right, 1, 0, 0)).unwrap();
    assert!(frame.pixel(798, 0));
    assert!(frame.pixel(799, 0));
}

#[test]
fn newline_and_wrapping_count_visual_lines() {
    let mut frame = Framebuffer::new();
    let layout = render_text(&mut frame, "A\nB", &BlockFont, style(Align::Left, 1, 0, 0)).unwrap();
    assert_eq!(layout.lines, 2);
    assert_eq!(layout.glyphs, 2);
    assert!(frame.pixel(0, 1));

    frame.clear(false);
    let text = String::from("A").repeat(801);
    let layout = render_text(&mut frame, &text, &BlockFont, style(Align::Left, 1, 0, 0)).unwrap();
    assert_eq!(layout.lines, 2);
    assert_eq!(layout.glyphs, 801);
    assert!(frame.pixel(0, 0));
    assert!(frame.pixel(799, 0));
    assert!(frame.pixel(0, 1));
}

#[test]
fn clipping_is_reported_and_bounded() {
    let mut frame = Framebuffer::new();
    let text = String::from("A\n").repeat(480) + "A";
    let layout = render_text(&mut frame, &text, &BlockFont, style(Align::Left, 1, 0, 0)).unwrap();
    assert_eq!(layout.lines, 480);
    assert_eq!(layout.glyphs, 480);
    assert!(layout.clipped);
}

#[test]
fn scale_two_doubles_each_glyph_pixel() {
    let mut frame = Framebuffer::new();
    let layout = render_text(&mut frame, "A", &BlockFont, style(Align::Left, 2, 0, 0)).unwrap();
    assert_eq!(layout.lines, 1);
    assert_eq!(layout.glyphs, 1);
    assert!(frame.pixel(0, 0));
    assert!(frame.pixel(1, 0));
    assert!(frame.pixel(0, 1));
    assert!(frame.pixel(1, 1));
    assert!(!frame.pixel(2, 2));
}

#[test]
fn unsupported_scale_is_an_error() {
    let mut frame = Framebuffer::new();
    assert_eq!(
        render_text(&mut frame, "A", &BlockFont, style(Align::Left, 3, 0, 0)),
        Err(LayoutError::UnsupportedScale)
    );
}
