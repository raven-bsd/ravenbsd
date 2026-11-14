// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn truecolor_golden() {
    let palette = Palette::raven_violet();
    assert_eq!(
        sgr(Role::Accent, &palette, Depth::TrueColor),
        "\x1b[38;2;157;124;255m"
    );
}

#[test]
fn ansi_256_colour_golden() {
    let palette = Palette::raven_violet();
    assert_eq!(
        sgr(Role::Accent, &palette, Depth::Ansi256),
        "\x1b[38;5;141m"
    );
}

#[test]
fn ansi_16_colour_golden() {
    let palette = Palette::raven_violet();
    assert_eq!(sgr(Role::Error, &palette, Depth::Ansi16), "\x1b[91m");
    assert_eq!(sgr(Role::Success, &palette, Depth::Ansi16), "\x1b[92m");
    assert_eq!(sgr(Role::Warning, &palette, Depth::Ansi16), "\x1b[93m");
    assert_eq!(sgr(Role::Info, &palette, Depth::Ansi16), "\x1b[94m");
    assert_eq!(sgr(Role::Accent, &palette, Depth::Ansi16), "\x1b[95m");
    assert_eq!(sgr(Role::Muted, &palette, Depth::Ansi16), "\x1b[90m");
}

#[test]
fn black_and_grey_use_the_grey_entries() {
    assert_eq!(ansi16(Rgb::new(0, 0, 0)), 0);
    assert_eq!(ansi16(Rgb::new(0x80, 0x80, 0x80)), 8);
    assert_eq!(ansi16(Rgb::new(0xff, 0xff, 0xff)), 15);
}

#[test]
fn disabled_paint_has_no_escape() {
    let palette = Palette::raven_violet();
    let text = paint(Role::Accent, "hello", &palette, Depth::TrueColor, false);
    assert_eq!(text, "hello");
    assert!(!text.contains('\x1b'));
}

#[test]
fn depth_detection() {
    assert_eq!(
        Depth::detect(Some("truecolor"), Some("xterm")),
        Depth::TrueColor
    );
    assert_eq!(Depth::detect(None, Some("xterm-256color")), Depth::Ansi256);
    assert_eq!(Depth::detect(None, Some("dumb")), Depth::Ansi16);
}
