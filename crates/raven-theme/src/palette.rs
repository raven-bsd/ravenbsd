// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The semantic roles and the built-in palettes.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// One semantic role. A role is a meaning, never a colour name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// Primary emphasis.
    Accent,
    /// Secondary/de-emphasised text.
    Muted,
    /// Borders and separators.
    Border,
    /// A successful result.
    Success,
    /// A warning.
    Warning,
    /// An error.
    Error,
    /// Informational text.
    Info,
    /// A filesystem path.
    Path,
    /// A numeric value.
    Number,
    /// A string literal.
    String,
    /// A keyword.
    Keyword,
}

impl Role {
    /// Every role, in display order.
    pub const ALL: [Self; 11] = [
        Self::Accent,
        Self::Muted,
        Self::Border,
        Self::Success,
        Self::Warning,
        Self::Error,
        Self::Info,
        Self::Path,
        Self::Number,
        Self::String,
        Self::Keyword,
    ];

    /// The stable role name used by JSON and TOML.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::Muted => "muted",
            Self::Border => "border",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Info => "info",
            Self::Path => "path",
            Self::Number => "number",
            Self::String => "string",
            Self::Keyword => "keyword",
        }
    }

    /// Parse a role name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == name)
    }

    /// The role's index in a palette array.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Accent => 0,
            Self::Muted => 1,
            Self::Border => 2,
            Self::Success => 3,
            Self::Warning => 4,
            Self::Error => 5,
            Self::Info => 6,
            Self::Path => 7,
            Self::Number => 8,
            Self::String => 9,
            Self::Keyword => 10,
        }
    }
}

/// One 24-bit sRGB colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// Construct a colour.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parse six hex digits, with or without a leading `#`.
    #[must_use]
    pub fn parse_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        if digits.len() != 6 || !digits.is_ascii() {
            return None;
        }
        let value = u32::from_str_radix(digits, 16).ok()?;
        Some(Self::new(
            u8::try_from(value >> 16).ok()?,
            u8::try_from((value >> 8) & 0xff).ok()?,
            u8::try_from(value & 0xff).ok()?,
        ))
    }

    /// The lowercase `#rrggbb` spelling.
    #[must_use]
    pub fn to_hex(self) -> String {
        let mut out = String::with_capacity(7);
        out.push('#');
        for byte in [self.r, self.g, self.b] {
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0xf));
        }
        out
    }

    /// WCAG relative luminance.
    #[must_use]
    pub fn luminance(self) -> f32 {
        fn channel(value: u8) -> f32 {
            // A monotonic gamma-2 approximation; the exact WCAG curve needs `libm`, which the
            // no_std core does not carry.
            let value = f32::from(value) / 255.0;
            value * value
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// WCAG contrast ratio against `other`.
    #[must_use]
    pub fn contrast(self, other: Self) -> f32 {
        let first = self.luminance();
        let second = other.luminance();
        let (high, low) = if first > second {
            (first, second)
        } else {
            (second, first)
        };
        (high + 0.05) / (low + 0.05)
    }
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        _ => char::from(b'a' + (value - 10)),
    }
}

/// One palette: a name, a background and one colour per role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Palette name.
    pub name: String,
    /// Background colour used for contrast validation.
    pub background: Rgb,
    roles: [Rgb; 11],
}

impl Palette {
    /// The Raven Violet default (dark).
    #[must_use]
    pub fn raven_violet() -> Self {
        Self {
            name: "raven-violet".to_string(),
            background: Rgb::new(0x17, 0x12, 0x24),
            roles: [
                Rgb::new(0x9d, 0x7c, 0xff),
                Rgb::new(0x8b, 0x8b, 0x9e),
                Rgb::new(0x6f, 0x5b, 0xd0),
                Rgb::new(0x7b, 0xd8, 0x8f),
                Rgb::new(0xf2, 0xc1, 0x4e),
                Rgb::new(0xff, 0x6b, 0x81),
                Rgb::new(0x62, 0xb6, 0xff),
                Rgb::new(0xc4, 0xa7, 0xff),
                Rgb::new(0xf5, 0xa9, 0x7f),
                Rgb::new(0xa6, 0xe3, 0xa1),
                Rgb::new(0xff, 0x9e, 0xcd),
            ],
        }
    }

    /// The monochrome fallback.
    #[must_use]
    pub fn mono() -> Self {
        Self {
            name: "mono".to_string(),
            background: Rgb::new(0, 0, 0),
            roles: [
                Rgb::new(0xff, 0xff, 0xff),
                Rgb::new(0xb0, 0xb0, 0xb0),
                Rgb::new(0x80, 0x80, 0x80),
                Rgb::new(0xff, 0xff, 0xff),
                Rgb::new(0xd0, 0xd0, 0xd0),
                Rgb::new(0xff, 0xff, 0xff),
                Rgb::new(0xc0, 0xc0, 0xc0),
                Rgb::new(0xff, 0xff, 0xff),
                Rgb::new(0xd8, 0xd8, 0xd8),
                Rgb::new(0xe8, 0xe8, 0xe8),
                Rgb::new(0xff, 0xff, 0xff),
            ],
        }
    }

    /// The classic 16-colour-inspired palette.
    #[must_use]
    pub fn classic() -> Self {
        Self {
            name: "classic".to_string(),
            background: Rgb::new(0, 0, 0),
            roles: [
                Rgb::new(0x55, 0x55, 0xff),
                Rgb::new(0xaa, 0xaa, 0xaa),
                Rgb::new(0x55, 0x55, 0x55),
                Rgb::new(0x55, 0xff, 0x55),
                Rgb::new(0xff, 0xff, 0x55),
                Rgb::new(0xff, 0x55, 0x55),
                Rgb::new(0x55, 0xff, 0xff),
                Rgb::new(0x55, 0xaa, 0xff),
                Rgb::new(0xff, 0xaa, 0x55),
                Rgb::new(0xaa, 0xff, 0xaa),
                Rgb::new(0xff, 0x55, 0xff),
            ],
        }
    }

    /// A palette by built-in name.
    #[must_use]
    pub fn builtin(name: &str) -> Option<Self> {
        match name {
            "raven-violet" => Some(Self::raven_violet()),
            "mono" => Some(Self::mono()),
            "classic" => Some(Self::classic()),
            _ => None,
        }
    }

    /// The colour of one role.
    #[must_use]
    pub fn get(&self, role: Role) -> Rgb {
        self.roles[role.index()]
    }

    /// Replace one role.
    pub fn set(&mut self, role: Role, colour: Rgb) {
        self.roles[role.index()] = colour;
    }

    /// The roles in display order.
    #[must_use]
    pub fn roles(&self) -> Vec<(Role, Rgb)> {
        Role::ALL
            .into_iter()
            .map(|role| (role, self.get(role)))
            .collect()
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::raven_violet()
    }
}

#[cfg(test)]
#[path = "palette_ut.rs"]
mod tests;
