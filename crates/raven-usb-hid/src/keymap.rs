// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! HID usage-to-ASCII map for the US keyboard layout.

/// First HID usage identifier of the left/right modifier byte (Left Control).
pub const MODIFIER_FIRST: u8 = 0xe0;
/// Last HID usage identifier of the modifier byte (Right GUI).
pub const MODIFIER_LAST: u8 = 0xe7;
/// HID usage identifier of Caps Lock.
pub const CAPS_LOCK: u8 = 0x39;

/// True when `usage` is a keyboard modifier bit.
#[must_use]
pub const fn is_modifier(usage: u8) -> bool {
    usage >= MODIFIER_FIRST && usage <= MODIFIER_LAST
}

/// Translate one HID keyboard usage to ASCII.
///
/// Letters use `shift XOR caps`; punctuation uses the shifted table when `shift` is held.
/// Modifiers, Caps Lock, arrows and function keys return `None`.
#[must_use]
pub fn key_to_ascii(usage: u8, shift: bool, caps: bool) -> Option<u8> {
    let base = match usage {
        0x04..=0x1d => {
            let letter = b'a' + (usage - 0x04);
            return Some(if shift ^ caps {
                letter.to_ascii_uppercase()
            } else {
                letter
            });
        }
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
        0x31 => b'\\',
        0x32 => b'#',
        0x33 => b';',
        0x34 => b'\'',
        0x35 => b'`',
        0x36 => b',',
        0x37 => b'.',
        0x38 => b'/',
        _ => return None,
    };
    if !shift {
        return Some(base);
    }
    Some(match usage {
        0x1e => b'!',
        0x1f => b'@',
        0x20 => b'#',
        0x21 => b'$',
        0x22 => b'%',
        0x23 => b'^',
        0x24 => b'&',
        0x25 => b'*',
        0x26 => b'(',
        0x27 => b')',
        0x2d => b'_',
        0x2e => b'+',
        0x2f => b'{',
        0x30 => b'}',
        0x31 => b'|',
        0x33 => b':',
        0x34 => b'"',
        0x35 => b'~',
        0x36 => b'<',
        0x37 => b'>',
        0x38 => b'?',
        _ => base,
    })
}

#[cfg(test)]
#[path = "keymap_ut.rs"]
mod tests;
