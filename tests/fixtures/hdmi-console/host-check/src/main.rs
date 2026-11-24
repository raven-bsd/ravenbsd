// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K386 host check for the HDMI text console.
//!
//! This program does not touch QEMU or a Pi. It proves the parts a QEMU serial transcript
//! cannot show directly:
//!
//! * `xtask/src/pi.rs` still configures the firmware display buffer at 1024x768x32;
//! * the merged `raven-fb` blitter, cursor and scroll produce exact pixels at that mode;
//! * the mailbox codec rejects malformed/failed replies without panicking;
//! * the kernel `fb.rs` failure paths stay UART-only, and `serial.rs` still mirrors both
//!   `write_str` and `write_bytes` to `fb::write_text`.
//!
//! It also writes a synthetic 1024x768 PPM of the target styled boot screen plus the login
//! prompt. That artifact is a blitter render of the same bytes, not a physical monitor capture.

use std::fs::{self, File};
use std::io::{BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use raven_fb::mailbox::{self, MailboxError};
use raven_fb::{Color, DisplayRequest, PixelFormat, ScreenBuffer, TextConsole, glyph};

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 768;
const BITS: u32 = 32;
const BYTES_PER_PIXEL: u32 = 4;

const TITLE: &str = "R A V E N   B S D";
const RULE: &str =
    "--------------------------------------------------------------------------------";
const WELCOME: &str = "Welcome to RavenBSD 0.1 aarch64";
const LOGIN: &str = "login: ";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hdmi-console-host-check: FAIL {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let root = PathBuf::from(".");
    let config = extract_config(&read(&root.join("xtask/src/pi.rs"))?)?;
    check_config(&config)?;

    let width = config_value(&config, "framebuffer_width")?;
    let height = config_value(&config, "framebuffer_height")?;
    let depth = config_value(&config, "framebuffer_depth")?;
    if (width, height, depth) != (WIDTH, HEIGHT, BITS) {
        return Err(format!(
            "config mode {width}x{height}x{depth} is not the console mode {WIDTH}x{HEIGHT}x{BITS}"
        ));
    }
    let request = DisplayRequest::default();
    if (request.width, request.height, request.depth) != (width, height, depth) {
        return Err(format!(
            "DisplayRequest::default is {}x{}x{}, config is {width}x{height}x{depth}",
            request.width, request.height, request.depth
        ));
    }

    fs::create_dir_all("build/fixtures/hdmi-console")
        .map_err(|error| format!("cannot create fixture output dir: {error}"))?;
    fs::create_dir_all("build/qtest")
        .map_err(|error| format!("cannot create qtest output dir: {error}"))?;
    fs::write("build/fixtures/hdmi-console/config.txt", &config)
        .map_err(|error| format!("cannot write config.txt: {error}"))?;

    check_source_invariants(&root)?;
    check_mailbox()?;
    let summary = render_artifacts(width, height)?;
    let log = format!(
        "hdmi-console-host-check: PASS\nmode={width}x{height}x{depth}\nrequest={}x{}x{}\n{summary}",
        request.width, request.height, request.depth
    );
    fs::write("build/fixtures/hdmi-console/host.log", &log)
        .map_err(|error| format!("cannot write host.log: {error}"))?;
    print!("{log}");
    Ok(())
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Extract the `CONFIG_TXT` string literal from `xtask/src/pi.rs`.
fn extract_config(source: &str) -> Result<String, String> {
    let marker = "const CONFIG_TXT: &str = \"";
    let start = source
        .find(marker)
        .ok_or_else(|| "xtask/src/pi.rs has no CONFIG_TXT literal".to_string())?
        + marker.len();
    let mut out = String::new();
    let mut chars = source[start..].chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' => return Ok(out),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\n') => {
                    while matches!(chars.peek(), Some(' ' | '\t' | '\r' | '\n')) {
                        chars.next();
                    }
                }
                Some(other) => return Err(format!("unsupported CONFIG_TXT escape \\{other}")),
                None => return Err("unterminated CONFIG_TXT escape".to_string()),
            },
            other => out.push(other),
        }
    }
    Err("unterminated CONFIG_TXT literal".to_string())
}

fn check_config(config: &str) -> Result<(), String> {
    for expected in [
        "framebuffer_width=1024",
        "framebuffer_height=768",
        "framebuffer_depth=32",
        "framebuffer_ignore_alpha=1",
        "hdmi_force_hotplug=1",
        "hdmi_group=2",
        "hdmi_mode=16",
        "disable_overscan=1",
        // UART remains the recovery console and is never disabled by the HDMI mode.
        "enable_uart=1",
        "uart_2ndstage=1",
    ] {
        if !config.lines().any(|line| line.trim() == expected) {
            return Err(format!("config.txt is missing `{expected}`"));
        }
    }
    Ok(())
}

fn config_value(config: &str, key: &str) -> Result<u32, String> {
    let prefix = format!("{key}=");
    let value = config
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .ok_or_else(|| format!("config.txt is missing `{key}`"))?;
    value
        .parse()
        .map_err(|error| format!("config.txt `{key}={value}`: {error}"))
}

/// The failure paths this ticket promises: every mailbox/geometry failure logs a UART-only
/// fallback and returns, and both console write paths mirror to the display console.
fn check_source_invariants(root: &Path) -> Result<(), String> {
    let fb = read(&root.join("kernel/src/arch/aarch64/fb.rs"))?;
    for expected in [
        "fb: mailbox timeout; UART only",
        "fb: response rejected: {error:?}; UART only",
        "fb: depth {} unsupported; UART only",
        "fb: display buffer outside the mapped RAM window",
        "fb: property buffer outside SDRAM",
        "fb: encode failed: {error:?}",
    ] {
        if !fb.contains(expected) {
            return Err(format!("kernel fb.rs is missing fallback `{expected}`"));
        }
    }
    if fb.contains("panic!(") || fb.contains(".unwrap()") || fb.contains(".expect(") {
        return Err("kernel fb.rs contains a panic/unwrap/expect path".to_string());
    }

    let serial = read(&root.join("kernel/src/arch/aarch64/serial.rs"))?;
    for expected in [
        "super::fb::write_text(text.as_bytes());",
        "super::fb::write_text(bytes);",
    ] {
        if !serial.contains(expected) {
            return Err(format!("serial.rs is missing the HDMI mirror `{expected}`"));
        }
    }

    let text = read(&root.join("crates/raven-fb/src/text.rs"))?;
    if !text.contains("fn scroll(") || !text.contains("copy_within(") {
        return Err("raven-fb text.rs no longer scrolls the display buffer".to_string());
    }
    Ok(())
}

fn check_mailbox() -> Result<(), String> {
    let mut short = [0u8; 8];
    if mailbox::encode_request(&mut short, &DisplayRequest::default())
        != Err(MailboxError::BufferTooSmall)
    {
        return Err("short mailbox request did not return BufferTooSmall".to_string());
    }
    if mailbox::decode_response(&[]) != Err(MailboxError::MalformedResponse) {
        return Err("empty mailbox response did not return MalformedResponse".to_string());
    }
    let mut buffer = [0u8; mailbox::MAX_PROPERTY_BYTES];
    let length = mailbox::encode_request(&mut buffer, &DisplayRequest::default())
        .map_err(|error| format!("valid mailbox request failed: {error:?}"))?;
    // A request header has code 0; a firmware failure is any code without the response bit.
    if mailbox::decode_response(&buffer[..length]) != Err(MailboxError::FirmwareError) {
        return Err("failed mailbox reply did not return FirmwareError".to_string());
    }
    Ok(())
}

fn render_artifacts(width: u32, height: u32) -> Result<String, String> {
    let pitch = width
        .checked_mul(BYTES_PER_PIXEL)
        .ok_or_else(|| "pitch overflow".to_string())?;
    let length = usize::try_from(
        pitch
            .checked_mul(height)
            .ok_or_else(|| "framebuffer size overflow".to_string())?,
    )
    .map_err(|_| "framebuffer size does not fit usize".to_string())?;
    let mut pixels = vec![0u8; length];
    let mut screen = ScreenBuffer {
        pixels: &mut pixels,
        width,
        height,
        pitch,
        format: PixelFormat::Rgb888,
    };
    let mut console = TextConsole::new(&screen).map_err(|error| format!("console: {error:?}"))?;
    console
        .clear(&mut screen)
        .map_err(|error| format!("clear: {error:?}"))?;
    let screen_text = format!(
        "{TITLE}\n{RULE}\n000.00  svcd                ok\n000.00  consd               ok\n\
         000.00  getty               ok\n{RULE}\n\n{WELCOME}\n{LOGIN}"
    );
    console
        .write(&mut screen, screen_text.as_bytes())
        .map_err(|error| format!("boot screen: {error:?}"))?;
    if console.columns() != width / 8 || console.rows() != height / 16 {
        return Err("console geometry does not match 8x16 cells".to_string());
    }
    if console.cursor() != (7, 8) {
        return Err(format!(
            "boot screen cursor is {:?}, expected (7, 8)",
            console.cursor()
        ));
    }
    if !cell_matches(&screen, 0, 0, b'R', 16) {
        return Err("title glyph 'R' is not the exact 8x16 bitmap".to_string());
    }
    if !cell_matches(&screen, 0, 1, b'-', 16) {
        return Err("rule glyph '-' is not the exact 8x16 bitmap".to_string());
    }
    if !cell_matches(&screen, 80, 1, b' ', 16) {
        return Err("the rule is not 80 columns wide".to_string());
    }

    let mut scroll_pixels = vec![0u8; length];
    let mut scroll_screen = ScreenBuffer {
        pixels: &mut scroll_pixels,
        width,
        height,
        pitch,
        format: PixelFormat::Rgb888,
    };
    let mut scroll_console =
        TextConsole::new(&scroll_screen).map_err(|error| format!("scroll console: {error:?}"))?;
    scroll_console
        .clear(&mut scroll_screen)
        .map_err(|error| format!("scroll clear: {error:?}"))?;
    for index in 0..50 {
        let line = format!("line-{index:02}\n");
        scroll_console
            .write(&mut scroll_screen, line.as_bytes())
            .map_err(|error| format!("scroll line {index}: {error:?}"))?;
    }
    if scroll_console.cursor() != (0, 47) {
        return Err(format!(
            "scroll cursor is {:?}, expected (0, 47)",
            scroll_console.cursor()
        ));
    }
    for (column, byte) in b"line-03".iter().copied().enumerate() {
        let column = u32::try_from(column).map_err(|_| "column overflow".to_string())?;
        if !cell_matches(&scroll_screen, column, 0, byte, 14) {
            return Err("scroll did not move `line-03` to the top row".to_string());
        }
    }

    write_ppm(Path::new("build/qtest/hdmi-console.ppm"), &screen)?;
    fs::write("build/qtest/hdmi-console.fb.txt", &screen_text)
        .map_err(|error| format!("cannot write fb.txt: {error}"))?;
    Ok(format!(
        "blitter=ok scroll=ok cursor=ok rule_columns=80 artifact=build/qtest/hdmi-console.ppm\n"
    ))
}

fn cell_matches(screen: &ScreenBuffer<'_>, column: u32, row: u32, byte: u8, rows: usize) -> bool {
    let sample = glyph(byte);
    let left = column.saturating_mul(8);
    let top = row.saturating_mul(16);
    for y in 0..rows {
        let Some(bits) = sample.data.get(y) else {
            return false;
        };
        for x in 0..8 {
            let expected = if bits & (0x80 >> x) != 0 {
                Color::White
            } else {
                Color::Black
            };
            let pixel_x = left + x;
            let pixel_y = top + u32::try_from(y).unwrap_or(u32::MAX);
            if pixel_at(screen, pixel_x, pixel_y) != Some(expected) {
                return false;
            }
        }
    }
    true
}

fn pixel_at(screen: &ScreenBuffer<'_>, x: u32, y: u32) -> Option<Color> {
    let bytes = usize::try_from(screen.format.bytes_per_pixel()).ok()?;
    let offset = usize::try_from(y)
        .ok()?
        .checked_mul(usize::try_from(screen.pitch).ok()?)?
        .checked_add(usize::try_from(x).ok()?.checked_mul(bytes)?)?;
    let slot = screen.pixels.get(offset..offset.checked_add(bytes)?)?;
    let value = match bytes {
        4 => u32::from_le_bytes(slot.try_into().ok()?),
        2 => u32::from(u16::from_le_bytes(slot.try_into().ok()?)),
        _ => u32::from(*slot.first()?),
    };
    match value {
        0 => Some(Color::Black),
        other if other == screen.format.encode(Color::White) => Some(Color::White),
        _ => None,
    }
}

fn write_ppm(path: &Path, screen: &ScreenBuffer<'_>) -> Result<(), String> {
    let file = File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut writer = BufWriter::new(file);
    writeln!(writer, "P6").map_err(|error| error.to_string())?;
    writeln!(writer, "{} {}", screen.width, screen.height).map_err(|error| error.to_string())?;
    writeln!(writer, "255").map_err(|error| error.to_string())?;
    for y in 0..screen.height {
        for x in 0..screen.width {
            let color = pixel_at(screen, x, y).unwrap_or(Color::Black);
            let value = screen.format.encode(color).to_le_bytes();
            writer
                .write_all(&value[..3])
                .map_err(|error| error.to_string())?;
        }
    }
    writer.flush().map_err(|error| error.to_string())?;
    Ok(())
}
