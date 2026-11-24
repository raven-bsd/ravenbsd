// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K386 target probe for the HDMI text console.
//!
//! The service reads the exact firmware `config.txt` text staged into the test initfs, checks
//! the configured mode, drives the merged `raven-fb` blitter on a small local display buffer,
//! and proves the mailbox codec error paths return instead of panicking. It then prints the
//! target styled boot screen through the same `DEBUG_WRITE` path whose Pi 4 kernel glue mirrors
//! bytes to the HDMI console; the host check renders the same text to a 1024x768 PPM.
//!
//! The small local buffer is deliberate: the full 1024x768 artifact is rendered by the host
//! check, while this target probe proves the same code paths compile and run in the guest.
//! Physical panel evidence remains UI1's board run; this probe never claims a monitor.

#![no_std]
#![no_main]

use raven_fb::mailbox::{self, MailboxError};
use raven_fb::{Color, DisplayRequest, PixelFormat, ScreenBuffer, TextConsole, glyph};
use raven_initfs::Initfs;
use raven_rt::entry;

entry!(main);

const CONFIG_PATH: &str = "/etc/hdmi-console/config.txt";
const WIDTH: u32 = 64;
const HEIGHT: u32 = 32;
const BYTES_PER_PIXEL: u32 = 4;
const PITCH: u32 = WIDTH * BYTES_PER_PIXEL;
const PIXELS: usize = (PITCH * HEIGHT) as usize;
const RULE: &str =
    "--------------------------------------------------------------------------------";
#[cfg(target_arch = "x86_64")]
const ARCH: &str = "x86_64";
#[cfg(target_arch = "aarch64")]
const ARCH: &str = "aarch64";

fn main(_args: raven_rt::Args) -> i32 {
    let config = match read_config() {
        Some(config) => config,
        None => {
            raven_rt::println!("hdmi-console: FAIL config unreadable");
            return 1;
        }
    };
    let width = match value(&config, "framebuffer_width").and_then(parse_u32) {
        Some(width) => width,
        None => return fail("config width"),
    };
    let height = match value(&config, "framebuffer_height").and_then(parse_u32) {
        Some(height) => height,
        None => return fail("config height"),
    };
    let depth = match value(&config, "framebuffer_depth").and_then(parse_u32) {
        Some(depth) => depth,
        None => return fail("config depth"),
    };
    if (width, height, depth) != (1024, 768, 32) {
        raven_rt::println!("hdmi-console: FAIL mode {width}x{height}x{depth}");
        return 1;
    }
    let request = DisplayRequest::default();
    if (request.width, request.height, request.depth) != (width, height, depth) {
        return fail("display request mode");
    }
    if PixelFormat::from_depth(depth) != Some(PixelFormat::Rgb888) {
        return fail("pixel format");
    }
    raven_rt::println!("hdmi-console: mode {width}x{height}x{depth} ok");

    if !blit_check() {
        return fail("blitter");
    }
    raven_rt::println!("hdmi-console: blit ok");
    if !scroll_check() {
        return fail("scroll");
    }
    raven_rt::println!("hdmi-console: scroll ok");
    if !mailbox_check() {
        return fail("mailbox fallback");
    }
    raven_rt::println!("hdmi-console: mailbox fallback ok");
    print_styled_screen();
    raven_rt::println!("hdmi-console: done");
    0
}

/// Print the target styled boot screen to the serial mirror.
///
/// These are the text lines the host check renders at 1024x768. The live init on the base tree
/// still prints its concise boot report; K374 owns the live presentation. The marker keeps the
/// probe's lines distinct from the init output while the login prompt remains getty's real one.
fn print_styled_screen() {
    raven_rt::println!("hdmi-console: styled screen");
    raven_rt::println!("R A V E N   B S D");
    raven_rt::println!("{RULE}");
    raven_rt::println!("000.00  svcd                ok");
    raven_rt::println!("000.00  consd               ok");
    raven_rt::println!("000.00  getty               ok");
    raven_rt::println!("{RULE}");
    raven_rt::println!();
    raven_rt::println!("Welcome to RavenBSD 0.1 {ARCH}");
}

fn fail(what: &str) -> i32 {
    raven_rt::println!("hdmi-console: FAIL {what}");
    1
}

fn read_config() -> Option<&'static str> {
    let image = raven_rt::map_initfs().ok()?;
    let initfs = Initfs::parse(image).ok()?;
    let entry = initfs.get(CONFIG_PATH)?;
    core::str::from_utf8(entry.data).ok()
}

fn value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
    let mut lines = config.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key)
            && let Some(value) = rest.strip_prefix('=')
        {
            return Some(value.trim());
        }
    }
    None
}

fn parse_u32(text: &str) -> Option<u32> {
    text.parse().ok()
}

fn blit_check() -> bool {
    let mut pixels = [0u8; PIXELS];
    let mut screen = screen(&mut pixels);
    let mut console = match TextConsole::new(&screen) {
        Ok(console) => console,
        Err(_) => return false,
    };
    if console.clear(&mut screen).is_err() || console.write(&mut screen, b"RavenBSD").is_err() {
        return false;
    }
    // The first cell is 'R'; compare every pixel of its 8x16 bitmap.
    let sample = glyph(b'R');
    for (row, bits) in sample.data.iter().enumerate() {
        for column in 0..8 {
            let expected = if bits & (0x80 >> column) != 0 {
                Color::White
            } else {
                Color::Black
            };
            let y = u32::try_from(row).unwrap_or(u32::MAX);
            let x = u32::try_from(column).unwrap_or(u32::MAX);
            if pixel_at(&screen, x, y) != Some(expected) {
                return false;
            }
        }
    }
    // The character after the eight-column line is on the next row; the console must be 8x2.
    console.columns() == 8 && console.rows() == 2
}

fn scroll_check() -> bool {
    let mut pixels = [0u8; PIXELS];
    let mut screen = screen(&mut pixels);
    let mut console = match TextConsole::new(&screen) {
        Ok(console) => console,
        Err(_) => return false,
    };
    if console.clear(&mut screen).is_err() || console.write(&mut screen, b"A\nB\nC\n").is_err() {
        return false;
    }
    // Two rows scroll twice: row 0 is C and the cursor sits on the cleared last row.
    cell_matches(&screen, 0, 0, b'C') && console.cursor() == (0, 1)
}

fn mailbox_check() -> bool {
    let mut short = [0u8; 8];
    if mailbox::encode_request(&mut short, &DisplayRequest::default())
        != Err(MailboxError::BufferTooSmall)
    {
        return false;
    }
    if mailbox::decode_response(&[]) != Err(MailboxError::MalformedResponse) {
        return false;
    }
    let mut buffer = [0u8; mailbox::MAX_PROPERTY_BYTES];
    let Ok(length) = mailbox::encode_request(&mut buffer, &DisplayRequest::default()) else {
        return false;
    };
    // An encoded request has code 0; decoding it is the failed-reply path.
    mailbox::decode_response(&buffer[..length]) == Err(MailboxError::FirmwareError)
}

fn screen(pixels: &mut [u8; PIXELS]) -> ScreenBuffer<'_> {
    ScreenBuffer {
        pixels,
        width: WIDTH,
        height: HEIGHT,
        pitch: PITCH,
        format: PixelFormat::Rgb888,
    }
}

fn cell_matches(screen: &ScreenBuffer<'_>, column: u32, row: u32, byte: u8) -> bool {
    let sample = glyph(byte);
    let left = column.saturating_mul(8);
    let top = row.saturating_mul(16);
    for (offset, bits) in sample.data.iter().enumerate() {
        for x in 0..8 {
            let expected = if bits & (0x80 >> x) != 0 {
                Color::White
            } else {
                Color::Black
            };
            let pixel_x = left.saturating_add(x);
            let pixel_y = top.saturating_add(u32::try_from(offset).unwrap_or(u32::MAX));
            if pixel_at(screen, pixel_x, pixel_y) != Some(expected) {
                return false;
            }
        }
    }
    true
}

fn pixel_at(screen: &ScreenBuffer<'_>, x: u32, y: u32) -> Option<Color> {
    let offset = usize::try_from(y)
        .ok()?
        .checked_mul(usize::try_from(screen.pitch).ok()?)?
        .checked_add(usize::try_from(x).ok()?.checked_mul(4)?)?;
    let slot = screen.pixels.get(offset..offset.checked_add(4)?)?;
    match u32::from_le_bytes(slot.try_into().ok()?) {
        0 => Some(Color::Black),
        value if value == screen.format.encode(Color::White) => Some(Color::White),
        _ => None,
    }
}
