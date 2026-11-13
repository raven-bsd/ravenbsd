// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent per-code-point Unicode oracle for the tests.
//!
//! This reader is deliberately separate from `tests/support/gen.rs`: it parses the same UCD
//! files into flat per-code-point arrays and implements the class/width rules again, so a bug in
//! the table generator or in the Rust lookup cannot hide behind the same code.

use std::fs;
use std::path::Path;

const MAX_CP: usize = 0x11_0000;

const GC_P: u8 = 1;
const GC_S: u8 = 2;
const GC_M: u8 = 4;
const GC_ND: u8 = 8;
const GC_MN: u8 = 16;
const GC_ME: u8 = 32;
const GC_CF: u8 = 64;
const GC_NO: u8 = 128;

/// Independent Unicode view of every code point.
pub struct Oracle {
    /// `DerivedCoreProperties` `Alphabetic`.
    pub alphabetic: Vec<bool>,
    /// General-category class bits.
    pub category: Vec<u8>,
    /// `East_Asian_Width` `W` or `F`.
    pub wide: Vec<bool>,
    /// Simple uppercase mapping (`identity` above `0x1FFFF`).
    pub upper: Vec<u32>,
    /// Simple lowercase mapping.
    pub lower: Vec<u32>,
    /// `PropList` `White_Space`.
    pub white_space: Vec<bool>,
    /// `emoji-data` `Emoji_Presentation`.
    pub emoji_presentation: Vec<bool>,
}

fn hex(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim(), 16).ok()
}

fn range(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    match text.split_once("..") {
        Some((start, end)) => Some((hex(start)?, hex(end)?)),
        None => {
            let value = hex(text)?;
            Some((value, value))
        }
    }
}

fn category_bits(name: &str) -> u8 {
    match name.as_bytes() {
        b"Nd" => GC_ND,
        b"Mn" => GC_MN,
        b"Me" => GC_ME,
        b"Cf" => GC_CF,
        b"No" => GC_NO,
        _ => match name.as_bytes().first() {
            Some(b'P') => GC_P,
            Some(b'S') => GC_S,
            Some(b'M') => GC_M,
            _ => 0,
        },
    }
}

/// Load the six vendored files.
///
/// # Errors
///
/// A missing or unreadable file.
pub fn load(ucd: &Path) -> Result<Oracle, String> {
    let unicode_data = fs::read_to_string(ucd.join("UnicodeData.txt"))
        .map_err(|error| format!("cannot read UnicodeData.txt: {error}"))?;
    let derived = fs::read_to_string(ucd.join("DerivedCoreProperties.txt"))
        .map_err(|error| format!("cannot read DerivedCoreProperties.txt: {error}"))?;
    let east_asian = fs::read_to_string(ucd.join("EastAsianWidth.txt"))
        .map_err(|error| format!("cannot read EastAsianWidth.txt: {error}"))?;
    let prop_list = fs::read_to_string(ucd.join("PropList.txt"))
        .map_err(|error| format!("cannot read PropList.txt: {error}"))?;
    let emoji = fs::read_to_string(ucd.join("emoji-data.txt"))
        .map_err(|error| format!("cannot read emoji-data.txt: {error}"))?;

    let mut oracle = Oracle {
        alphabetic: vec![false; MAX_CP],
        category: vec![0; MAX_CP],
        wide: vec![false; MAX_CP],
        upper: (0..MAX_CP as u32).collect(),
        lower: (0..MAX_CP as u32).collect(),
        white_space: vec![false; MAX_CP],
        emoji_presentation: vec![false; MAX_CP],
    };

    let mut pending: Option<(u32, u8)> = None;
    for line in unicode_data.lines() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() < 15 {
            continue;
        }
        let Some(cp) = hex(fields[0]) else {
            continue;
        };
        if cp as usize >= MAX_CP {
            continue;
        }
        let name = fields[1];
        let bits = category_bits(fields[2]);
        if name.ends_with(", First>") {
            pending = Some((cp, bits));
            continue;
        }
        if name.ends_with(", Last>") {
            let Some((start, bits_first)) = pending.take() else {
                continue;
            };
            for code in start..=cp {
                oracle.category[code as usize] = bits_first;
            }
            continue;
        }
        oracle.category[cp as usize] = bits;
        if let Some(upper) = fields.get(12).and_then(|text| hex(text)) {
            oracle.upper[cp as usize] = upper;
        }
        if let Some(lower) = fields.get(13).and_then(|text| hex(text)) {
            oracle.lower[cp as usize] = lower;
        }
    }

    for line in derived.lines() {
        let data = line.split('#').next().unwrap_or("").trim();
        if data.is_empty() {
            continue;
        }
        let Some((codes, property)) = data.split_once(';') else {
            continue;
        };
        if property.trim() != "Alphabetic" {
            continue;
        }
        if let Some((start, end)) = range(codes) {
            for cp in start..=end.min(MAX_CP as u32 - 1) {
                oracle.alphabetic[cp as usize] = true;
            }
        }
    }

    for line in east_asian.lines() {
        let data = line.split('#').next().unwrap_or("").trim();
        if data.is_empty() {
            continue;
        }
        let Some((codes, width)) = data.split_once(';') else {
            continue;
        };
        if width.trim() != "W" && width.trim() != "F" {
            continue;
        }
        if let Some((start, end)) = range(codes) {
            for cp in start..=end.min(MAX_CP as u32 - 1) {
                oracle.wide[cp as usize] = true;
            }
        }
    }

    for line in prop_list.lines() {
        let data = line.split('#').next().unwrap_or("").trim();
        if data.is_empty() {
            continue;
        }
        let Some((codes, property)) = data.split_once(';') else {
            continue;
        };
        if property.trim() != "White_Space" {
            continue;
        }
        if let Some((start, end)) = range(codes) {
            for cp in start..=end.min(MAX_CP as u32 - 1) {
                oracle.white_space[cp as usize] = true;
            }
        }
    }

    for line in emoji.lines() {
        let data = line.split('#').next().unwrap_or("").trim();
        if data.is_empty() {
            continue;
        }
        let Some((codes, property)) = data.split_once(';') else {
            continue;
        };
        if property.trim() != "Emoji_Presentation" {
            continue;
        }
        if let Some((start, end)) = range(codes) {
            for cp in start..=end.min(MAX_CP as u32 - 1) {
                oracle.emoji_presentation[cp as usize] = true;
            }
        }
    }

    Ok(oracle)
}

impl Oracle {
    /// The 5.5 class rule for `iswalpha`.
    #[must_use]
    pub fn is_alpha(&self, cp: u32) -> bool {
        let index = cp as usize;
        self.alphabetic[index]
            || (self.category[index] & GC_ND != 0 && !(0x30..=0x39).contains(&cp))
    }

    /// The 5.C class rule for `iswpunct`.
    #[must_use]
    pub fn is_punct(&self, cp: u32) -> bool {
        let index = cp as usize;
        let bits = self.category[index];
        let mark = bits & (GC_M | GC_MN | GC_ME) != 0;
        bits & (GC_P | GC_S) != 0
            || (mark && !self.alphabetic[index])
            || (bits & GC_NO != 0 && !self.alphabetic[index])
            || matches!(cp, 0x00a0 | 0x00ad | 0x2007 | 0x202f)
    }

    fn is_zero(&self, cp: u32) -> bool {
        let bits = self.category[cp as usize];
        ((bits & (GC_MN | GC_ME | GC_CF)) != 0 && cp != 0x00ad)
            || (0x1160..=0x11ff).contains(&cp)
            || (0x200b..=0x200f).contains(&cp)
            || (0x2060..=0x2064).contains(&cp)
    }

    /// The `wcwidth` rule with independent width arrays.
    #[must_use]
    pub fn width(&self, cp: u32) -> i32 {
        if cp < 0xff {
            return if (cp.wrapping_add(1) & 0x7f) >= 0x21 {
                1
            } else if cp != 0 {
                -1
            } else {
                0
            };
        }
        if (cp & 0xfffe_ffff) < 0xfffe {
            if self.is_zero(cp) {
                return 0;
            }
            if self.wide[cp as usize] {
                return 2;
            }
            return 1;
        }
        if (cp & 0xfffe) == 0xfffe {
            return -1;
        }
        if cp.wrapping_sub(0x20000) < 0x20000 {
            return 2;
        }
        if cp == 0xe0001 || cp.wrapping_sub(0xe0020) <= 0x5f || cp.wrapping_sub(0xe0100) <= 0xef {
            return 0;
        }
        1
    }

    /// Simple uppercase, with the `>= 0x20000` identity rule.
    #[must_use]
    pub fn upper(&self, cp: u32) -> u32 {
        if cp >= 0x20000 {
            cp
        } else {
            self.upper[cp as usize]
        }
    }

    /// Simple lowercase, with the `>= 0x20000` identity rule.
    #[must_use]
    pub fn lower(&self, cp: u32) -> u32 {
        if cp >= 0x20000 {
            cp
        } else {
            self.lower[cp as usize]
        }
    }
}
