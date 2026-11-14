// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ANSI rendering with truecolor, 256-colour and 16-colour fallbacks.

use alloc::string::{String, ToString};

use crate::palette::{Palette, Rgb, Role};

/// The selected colour depth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Depth {
    /// 16-colour ANSI.
    #[default]
    Ansi16,
    /// xterm 256-colour.
    Ansi256,
    /// 24-bit `COLORTERM=truecolor`.
    TrueColor,
}

impl Depth {
    /// Detect the depth from `COLORTERM` and `TERM`.
    #[must_use]
    pub fn detect(colorterm: Option<&str>, term: Option<&str>) -> Self {
        if let Some(value) = colorterm
            && (value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit"))
        {
            return Self::TrueColor;
        }
        if term.is_some_and(|value| value.contains("256color")) {
            return Self::Ansi256;
        }
        Self::Ansi16
    }

    /// Parse a stable depth name.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "16" | "ansi16" => Some(Self::Ansi16),
            "256" | "ansi256" => Some(Self::Ansi256),
            "truecolor" | "24bit" => Some(Self::TrueColor),
            _ => None,
        }
    }
}

/// The opening SGR sequence for one role at one depth.
#[must_use]
pub fn sgr(role: Role, palette: &Palette, depth: Depth) -> String {
    let colour = palette.get(role);
    match depth {
        Depth::TrueColor => alloc::format!("\x1b[38;2;{};{};{}m", colour.r, colour.g, colour.b),
        Depth::Ansi256 => {
            let index = xterm256(colour);
            alloc::format!("\x1b[38;5;{index}m")
        }
        Depth::Ansi16 => {
            let index = ansi16(colour);
            if index >= 8 {
                alloc::format!("\x1b[{}m", 90 + index - 8)
            } else {
                alloc::format!("\x1b[{}m", 30 + index)
            }
        }
    }
}

/// Paint `text` with `role` when `enabled`; otherwise return it unchanged.
#[must_use]
pub fn paint(role: Role, text: &str, palette: &Palette, depth: Depth, enabled: bool) -> String {
    if !enabled {
        return text.to_string();
    }
    let mut out = sgr(role, palette, depth);
    out.push_str(text);
    out.push_str("\x1b[0m");
    out
}

/// The xterm 256-colour cube index for a 24-bit colour.
#[must_use]
pub fn xterm256(colour: Rgb) -> u8 {
    let r = (u16::from(colour.r) * 5 + 127) / 255;
    let g = (u16::from(colour.g) * 5 + 127) / 255;
    let b = (u16::from(colour.b) * 5 + 127) / 255;
    u8::try_from(16 + 36 * r + 6 * g + b).unwrap_or(16)
}

/// A basic 16-colour index: 0..7 normal, 8..15 bright.
///
/// The fallback maps by hue sector, not by nearest RGB: the pastel Raven Violet roles are
/// perceptually green or red even when a saturated 16-colour cube entry is numerically closer
/// to a grey. Low-saturation colours become the grey/white entries.
#[must_use]
pub fn ansi16(colour: Rgb) -> u8 {
    let r = i32::from(colour.r);
    let g = i32::from(colour.g);
    let b = i32::from(colour.b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta == 0 || delta * 100 < max * 15 {
        return match max {
            0x00..=0x4f => 0,
            0x50..=0x9f => 8,
            0xa0..=0xdf => 7,
            _ => 15,
        };
    }
    let mut hue = if max == r {
        60 * (g - b) / delta
    } else if max == g {
        120 + 60 * (b - r) / delta
    } else {
        240 + 60 * (r - g) / delta
    };
    if hue < 0 {
        hue += 360;
    }
    let base = if !(15..345).contains(&hue) {
        1
    } else if hue < 75 {
        3
    } else if hue < 165 {
        2
    } else if hue < 195 {
        6
    } else if hue < 255 {
        4
    } else {
        5
    };
    let index = if max >= 0x80 { base + 8 } else { base };
    u8::try_from(index).unwrap_or(7)
}

#[cfg(test)]
#[path = "ansi_ut.rs"]
mod tests;
