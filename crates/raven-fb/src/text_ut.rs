// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the text blitter, scrolling and cursor.

use std::vec;
use std::vec::Vec;

use super::*;

fn screen32(width: u32, height: u32) -> Vec<u8> {
    vec![0u8; usize::try_from(width * height * 4).expect("screen bytes")]
}

fn read_pixel(screen: &ScreenBuffer<'_>, x: u32, y: u32) -> u32 {
    let bytes = screen.format.bytes_per_pixel();
    let offset = (y * screen.pitch + x * bytes as u32) as usize;
    match bytes {
        4 => u32::from_le_bytes([
            screen.pixels[offset],
            screen.pixels[offset + 1],
            screen.pixels[offset + 2],
            screen.pixels[offset + 3],
        ]),
        2 => u32::from(u16::from_le_bytes([
            screen.pixels[offset],
            screen.pixels[offset + 1],
        ])),
        _ => u32::from(screen.pixels[offset]),
    }
}

fn cell_matches(screen: &ScreenBuffer<'_>, column: u32, row: u32, byte: u8) -> bool {
    let glyph = glyph(byte);
    for (y, bits) in glyph.data.iter().enumerate().take(FONT_HEIGHT - 2) {
        for x in 0..FONT_WIDTH {
            let expected = if *bits & (0x80 >> x) != 0 {
                screen.format.encode(Color::White)
            } else {
                0
            };
            let actual = read_pixel(
                screen,
                column * FONT_WIDTH as u32 + x as u32,
                row * FONT_HEIGHT as u32 + y as u32,
            );
            if actual != expected {
                return false;
            }
        }
    }
    true
}

#[test]
fn clear_paints_black_and_places_the_cursor() {
    let mut pixels = screen32(80, 32);
    let mut screen = ScreenBuffer {
        pixels: &mut pixels,
        width: 80,
        height: 32,
        pitch: 320,
        format: PixelFormat::Rgb888,
    };
    let mut console = TextConsole::new(&screen).expect("console");
    let dirty = console.clear(&mut screen).expect("clear");
    assert_eq!(dirty.first, 0);
    assert_eq!(dirty.last, 31);
    assert_eq!(console.cursor(), (0, 0));
    assert_eq!(read_pixel(&screen, 0, 14), 0x00ff_ffff);
}

#[test]
fn writing_advances_the_cursor_and_draws_the_glyph() {
    let mut pixels = screen32(80, 32);
    let mut screen = ScreenBuffer {
        pixels: &mut pixels,
        width: 80,
        height: 32,
        pitch: 320,
        format: PixelFormat::Rgb888,
    };
    let mut console = TextConsole::new(&screen).expect("console");
    console.clear(&mut screen).expect("clear");
    let dirty = console.write(&mut screen, b"A").expect("write");
    assert_eq!(console.cursor(), (1, 0));
    assert_eq!(dirty.first, 0);
    assert_eq!(dirty.last, 15);
    assert!(cell_matches(&screen, 0, 0, b'A'));
}

#[test]
fn newline_moves_to_the_next_row() {
    let mut pixels = screen32(80, 64);
    let mut screen = ScreenBuffer {
        pixels: &mut pixels,
        width: 80,
        height: 64,
        pitch: 320,
        format: PixelFormat::Rgb888,
    };
    let mut console = TextConsole::new(&screen).expect("console");
    console.clear(&mut screen).expect("clear");
    console.write(&mut screen, b"A\nB").expect("write");
    assert_eq!(console.cursor(), (1, 1));
    assert!(cell_matches(&screen, 0, 0, b'A'));
    assert!(cell_matches(&screen, 0, 1, b'B'));
}

#[test]
fn a_full_console_scrolls_and_keeps_the_last_line() {
    let mut pixels = screen32(80, 32);
    let mut screen = ScreenBuffer {
        pixels: &mut pixels,
        width: 80,
        height: 32,
        pitch: 320,
        format: PixelFormat::Rgb888,
    };
    let mut console = TextConsole::new(&screen).expect("console");
    console.clear(&mut screen).expect("clear");
    let dirty = console.write(&mut screen, b"A\nB\nC").expect("write");
    assert_eq!(console.cursor(), (1, 1));
    assert_eq!(dirty.first, 0);
    assert_eq!(dirty.last, 31);
    assert!(cell_matches(&screen, 0, 0, b'B'));
    assert!(cell_matches(&screen, 0, 1, b'C'));
}

#[test]
fn a_long_line_wraps() {
    let mut pixels = screen32(80, 32);
    let mut screen = ScreenBuffer {
        pixels: &mut pixels,
        width: 80,
        height: 32,
        pitch: 320,
        format: PixelFormat::Rgb888,
    };
    let mut console = TextConsole::new(&screen).expect("console");
    console.clear(&mut screen).expect("clear");
    console.write(&mut screen, b"0123456789A").expect("write");
    assert_eq!(console.cursor(), (1, 1));
}

#[test]
fn backspace_clears_the_previous_cell() {
    let mut pixels = screen32(80, 32);
    let mut screen = ScreenBuffer {
        pixels: &mut pixels,
        width: 80,
        height: 32,
        pitch: 320,
        format: PixelFormat::Rgb888,
    };
    let mut console = TextConsole::new(&screen).expect("console");
    console.clear(&mut screen).expect("clear");
    console.write(&mut screen, b"A\x08").expect("write");
    assert_eq!(console.cursor(), (0, 0));
    assert!(cell_matches(&screen, 0, 0, b' '));
}

#[test]
fn sixteen_and_eight_bit_screens_encode_white() {
    let mut pixels16 = vec![0u8; 80 * 32 * 2];
    let mut screen16 = ScreenBuffer {
        pixels: &mut pixels16,
        width: 80,
        height: 32,
        pitch: 160,
        format: PixelFormat::Rgb565,
    };
    let mut console = TextConsole::new(&screen16).expect("console");
    console.clear(&mut screen16).expect("clear");
    assert_eq!(read_pixel(&screen16, 0, 14), 0xffff);

    let mut pixels8 = vec![0u8; 80 * 32];
    let mut screen8 = ScreenBuffer {
        pixels: &mut pixels8,
        width: 80,
        height: 32,
        pitch: 80,
        format: PixelFormat::Indexed8,
    };
    let mut console = TextConsole::new(&screen8).expect("console");
    console.clear(&mut screen8).expect("clear");
    assert_eq!(read_pixel(&screen8, 0, 14), 0xff);
}

#[test]
fn a_short_screen_is_rejected() {
    let mut pixels = vec![0u8; 10];
    let screen = ScreenBuffer {
        pixels: &mut pixels,
        width: 80,
        height: 32,
        pitch: 320,
        format: PixelFormat::Rgb888,
    };
    assert_eq!(TextConsole::new(&screen), Err(TextError::ScreenTooSmall));
}
