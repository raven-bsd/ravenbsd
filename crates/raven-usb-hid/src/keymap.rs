// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! HID usage-to-ASCII maps for the US and UK keyboard layouts, with modifiers and locks.

/// First HID usage identifier of the left/right modifier byte (Left Control).
pub const MODIFIER_FIRST: u8 = 0xe0;
/// Last HID usage identifier of the modifier byte (Right GUI).
pub const MODIFIER_LAST: u8 = 0xe7;
/// HID usage identifier of Caps Lock.
pub const CAPS_LOCK: u8 = 0x39;
/// HID usage identifier of Num Lock.
pub const NUM_LOCK: u8 = 0x53;

/// Left Control modifier bit.
pub const LEFT_CTRL: u8 = 1 << 0;
/// Left Shift modifier bit.
pub const LEFT_SHIFT: u8 = 1 << 1;
/// Left Alt modifier bit.
pub const LEFT_ALT: u8 = 1 << 2;
/// Left GUI modifier bit.
pub const LEFT_GUI: u8 = 1 << 3;
/// Right Control modifier bit.
pub const RIGHT_CTRL: u8 = 1 << 4;
/// Right Shift modifier bit.
pub const RIGHT_SHIFT: u8 = 1 << 5;
/// Right Alt / AltGr modifier bit.
pub const RIGHT_ALT: u8 = 1 << 6;
/// Right GUI modifier bit.
pub const RIGHT_GUI: u8 = 1 << 7;

/// A supported keyboard layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// US English.
    #[default]
    Us,
    /// UK English.
    Uk,
}

impl Layout {
    /// The layout label used by the `dtrace`/driver output and tests.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Us => "us",
            Self::Uk => "uk",
        }
    }
}

/// True when `usage` is a keyboard modifier bit.
#[must_use]
pub const fn is_modifier(usage: u8) -> bool {
    usage >= MODIFIER_FIRST && usage <= MODIFIER_LAST
}

/// True when either Shift bit is held.
#[must_use]
pub const fn shift(modifiers: u8) -> bool {
    modifiers & (LEFT_SHIFT | RIGHT_SHIFT) != 0
}

/// True when either Control bit is held.
#[must_use]
pub const fn control(modifiers: u8) -> bool {
    modifiers & (LEFT_CTRL | RIGHT_CTRL) != 0
}

/// True when Left Alt is held.
#[must_use]
pub const fn alt(modifiers: u8) -> bool {
    modifiers & LEFT_ALT != 0
}

/// True when Right Alt (AltGr) is held.
#[must_use]
pub const fn altgr(modifiers: u8) -> bool {
    modifiers & RIGHT_ALT != 0
}

/// True when either GUI bit is held.
#[must_use]
pub const fn gui(modifiers: u8) -> bool {
    modifiers & (LEFT_GUI | RIGHT_GUI) != 0
}

/// Translate one HID keyboard usage to ASCII for the US layout.
///
/// This is the S60-compatible entry point; the full layout/modifier entry point is
/// [`key_to_ascii_layout`].
#[must_use]
pub fn key_to_ascii(usage: u8, shift: bool, caps: bool) -> Option<u8> {
    let modifiers = if shift { LEFT_SHIFT } else { 0 };
    key_to_ascii_layout(usage, Layout::Us, modifiers, caps, false)
}

/// Translate one HID keyboard usage with a layout, modifiers and locks.
///
/// Left Alt suppresses ASCII (application shortcuts); Right Alt is AltGr and, for US/UK, its
/// level-3 characters are non-ASCII or dead keys and therefore return `None` in the byte console
/// stream. Control applies the standard ASCII control mapping after the base/shift selected by the
/// layout and Caps Lock. Num Lock selects digits for the keypad; without it the keypad digits are
/// navigation keys with no ASCII.
#[must_use]
pub fn key_to_ascii_layout(
    usage: u8,
    layout: Layout,
    modifiers: u8,
    caps: bool,
    num_lock: bool,
) -> Option<u8> {
    if altgr(modifiers) {
        return level3(layout, usage);
    }
    if alt(modifiers) {
        return None;
    }
    let ascii = if (0x54..=0x63).contains(&usage) || usage == 0x67 {
        keypad(usage, num_lock)?
    } else if (0x04..=0x1d).contains(&usage) {
        let letter = base(layout, usage)?;
        if shift(modifiers) ^ caps {
            letter.to_ascii_uppercase()
        } else {
            letter
        }
    } else {
        let base = base(layout, usage)?;
        if shift(modifiers) {
            shifted(layout, usage, base)
        } else {
            base
        }
    };
    if control(modifiers) {
        return control_ascii(ascii);
    }
    Some(ascii)
}

/// Level-3 (AltGr) characters for US/UK. The ISO level-3 characters are non-ASCII (€, é, ...);
/// the byte console queue cannot represent them, so no US/UK AltGr combination yields a byte.
fn level3(_layout: Layout, _usage: u8) -> Option<u8> {
    None
}

/// Base (unshifted) usage map.
fn base(layout: Layout, usage: u8) -> Option<u8> {
    let value = match usage {
        0x04..=0x1d => b'a' + (usage - 0x04),
        0x1e..=0x26 => b'1' + (usage - 0x1e),
        0x27 => b'0',
        0x28 => b'\n',
        0x29 => 0x1b,
        0x2a => 0x08,
        0x2b => b'\t',
        0x2c => b' ',
        0x2d => b'-',
        0x2e => b'=',
        0x2f => b'[',
        0x30 => b']',
        0x31 if layout == Layout::Uk => b'#',
        0x31 => b'\\',
        0x32 => b'#',
        0x33 => b';',
        0x34 => b'\'',
        0x35 => b'`',
        0x36 => b',',
        0x37 => b'.',
        0x38 => b'/',
        0x64 => b'\\',
        _ => return None,
    };
    Some(value)
}

/// Shifted usage map, with the layout-specific ISO keys.
fn shifted(layout: Layout, usage: u8, base: u8) -> u8 {
    match (layout, usage) {
        (Layout::Uk, 0x1f) => b'"',
        (Layout::Uk, 0x20) => 0xa3, // £ (Latin-1)
        (Layout::Uk, 0x28) => b'@',
        (Layout::Uk, 0x31) => b'~',
        (Layout::Uk, 0x32) => b'~',
        (Layout::Uk, 0x64) => b'|',
        (Layout::Us, 0x31) => b'|',
        (Layout::Us, 0x32) => b'~',
        (Layout::Us, 0x64) => b'|',
        (_, 0x1e) => b'!',
        (_, 0x1f) => b'@',
        (_, 0x20) => b'#',
        (_, 0x21) => b'$',
        (_, 0x22) => b'%',
        (_, 0x23) => b'^',
        (_, 0x24) => b'&',
        (_, 0x25) => b'*',
        (_, 0x26) => b'(',
        (_, 0x27) => b')',
        (_, 0x2d) => b'_',
        (_, 0x2e) => b'+',
        (_, 0x2f) => b'{',
        (_, 0x30) => b'}',
        (_, 0x33) => b':',
        (_, 0x34) => b'"',
        (_, 0x35) => b'~',
        (_, 0x36) => b'<',
        (_, 0x37) => b'>',
        (_, 0x38) => b'?',
        _ => base,
    }
}

/// Keypad usages with Num Lock on.
fn keypad(usage: u8, num_lock: bool) -> Option<u8> {
    match usage {
        0x54 => Some(b'/'),
        0x55 => Some(b'*'),
        0x56 => Some(b'-'),
        0x57 => Some(b'+'),
        0x58 => Some(b'\n'),
        0x59..=0x61 if num_lock => Some(b'1' + (usage - 0x59)),
        0x62 if num_lock => Some(b'0'),
        0x63 if num_lock => Some(b'.'),
        0x67 => Some(b'='),
        _ => None,
    }
}

/// Standard ASCII control mapping.
fn control_ascii(ascii: u8) -> Option<u8> {
    if ascii.is_ascii_alphabetic() {
        return Some(
            ascii
                .to_ascii_lowercase()
                .wrapping_sub(b'a')
                .wrapping_add(1),
        );
    }
    Some(match ascii {
        b'[' | b'{' => 0x1b,
        b'\\' | b'|' => 0x1c,
        b']' | b'}' => 0x1d,
        b'^' | b'6' => 0x1e,
        b'_' | b'-' => 0x1f,
        b'@' | b'2' => 0x00,
        b'?' => 0x7f,
        other => other,
    })
}

#[cfg(test)]
#[path = "keymap_ut.rs"]
mod tests;
