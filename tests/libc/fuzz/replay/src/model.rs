// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reference models used by the target properties.
//!
//! The models are independent of the crates under test: a UTF-8 acceptance oracle, an integer
//! scanner over `u64`, a DNS name normaliser and the init-order checker.

/// UTF-8 acceptance: exactly `core::str::from_utf8`.
#[must_use]
pub fn utf8(bytes: &[u8]) -> Option<&str> {
    core::str::from_utf8(bytes).ok()
}

/// The result of the integer model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntModel {
    /// The signed value (`strtol`).
    pub signed: i64,
    /// The unsigned value (`strtoul`).
    pub unsigned: u64,
    /// Bytes consumed.
    pub consumed: usize,
    /// The magnitude overflowed the range; the value is saturated.
    pub range: bool,
    /// The base was invalid (`< 0`, `1` or `> 36`).
    pub invalid_base: bool,
    /// No digit was consumed.
    pub no_digits: bool,
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn value_of(byte: u8) -> u32 {
    digit(byte).unwrap_or(0)
}

fn digit(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte.wrapping_sub(b'0'))),
        b'a'..=b'z' => Some(u32::from(byte.wrapping_sub(b'a')).wrapping_add(10)),
        b'A'..=b'Z' => Some(u32::from(byte.wrapping_sub(b'A')).wrapping_add(10)),
        _ => None,
    }
}

/// The `strtol`/`strtoul` model: whitespace, sign, `0x`/`0b`/`0` prefix, digits, saturation.
#[must_use]
pub fn int(input: &[u8], base: i32) -> IntModel {
    let mut model = IntModel {
        signed: 0,
        unsigned: 0,
        consumed: 0,
        range: false,
        invalid_base: base < 0 || base == 1 || base > 36,
        no_digits: false,
    };
    if model.invalid_base {
        return model;
    }
    let mut index = 0usize;
    while input.get(index).is_some_and(|byte| is_space(*byte)) {
        index = index.saturating_add(1);
    }
    let mut negative = false;
    match input.get(index).copied() {
        Some(b'-') => {
            negative = true;
            index = index.saturating_add(1);
        }
        Some(b'+') => index = index.saturating_add(1),
        _ => {}
    }
    let mut radix = base;
    if radix == 0 || radix == 16 {
        let zero = input.get(index).copied() == Some(b'0');
        let marker = input.get(index.saturating_add(1)).copied().unwrap_or(0);
        let following = input.get(index.saturating_add(2)).copied().unwrap_or(0);
        if zero && matches!(marker, b'x' | b'X') && digit(following).is_some_and(|value| value < 16)
        {
            index = index.saturating_add(2);
            radix = 16;
        }
    }
    if radix == 0 || radix == 2 {
        let zero = input.get(index).copied() == Some(b'0');
        let marker = input.get(index.saturating_add(1)).copied().unwrap_or(0);
        let following = input.get(index.saturating_add(2)).copied().unwrap_or(0);
        if zero && matches!(marker, b'b' | b'B') && matches!(following, b'0' | b'1') {
            index = index.saturating_add(2);
            radix = 2;
        }
    }
    if radix == 0 {
        radix = if input.get(index).copied() == Some(b'0') {
            8
        } else {
            10
        };
    }
    let radix = u32::try_from(radix).unwrap_or(10);
    let mut magnitude: u128 = 0;
    let mut any = false;
    let mut overflow = false;
    while let Some(byte) = input.get(index).copied() {
        let Some(value) = digit(byte) else {
            break;
        };
        if value >= radix {
            break;
        }
        any = true;
        if !overflow {
            match magnitude
                .checked_mul(u128::from(radix))
                .and_then(|value| value.checked_add(u128::from(value_of(byte))))
            {
                Some(value) => magnitude = value,
                None => overflow = true,
            }
        }
        index = index.saturating_add(1);
    }
    if !any {
        model.no_digits = true;
        return model;
    }
    model.consumed = index;
    let signed_limit = if negative {
        1u128 << 63
    } else {
        (1u128 << 63) - 1
    };
    if magnitude > signed_limit {
        model.range = true;
        model.signed = if negative { i64::MIN } else { i64::MAX };
    } else {
        model.signed = if negative {
            (magnitude as i128).wrapping_neg() as i64
        } else {
            magnitude as i64
        };
    }
    if magnitude > u128::from(u64::MAX) {
        model.range = true;
        model.unsigned = u64::MAX;
    } else {
        let value = magnitude as u64;
        model.unsigned = if negative {
            value.wrapping_neg()
        } else {
            value
        };
    }
    model
}

/// The SysV `elf_hash` of the gABI definition.
#[must_use]
pub fn elf_hash(bytes: &[u8]) -> u32 {
    let mut hash = 0u32;
    for byte in bytes {
        hash = hash.wrapping_shl(4).wrapping_add(u32::from(*byte));
        let high = hash & 0xf000_0000;
        if high != 0 {
            hash ^= high >> 24;
        }
        hash &= !high;
    }
    hash
}

/// The GNU hash (djb2) of the gABI definition.
#[must_use]
pub fn gnu_hash(bytes: &[u8]) -> u32 {
    let mut hash = 5381u32;
    for byte in bytes {
        hash = hash.wrapping_mul(33).wrapping_add(u32::from(*byte));
    }
    hash
}

/// Lower-case an ASCII DNS name and drop one trailing dot.
#[must_use]
pub fn dn(name: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = name.iter().map(|byte| byte.to_ascii_lowercase()).collect();
    if out.last() == Some(&b'.') {
        out.pop();
    }
    out
}

/// Check that `order` contains each id once, dependencies first, and `fini` is its reverse.
#[must_use]
pub fn init_order_ok(order: &[u32], fini: &[u32], deps: &dyn Fn(u32) -> Vec<u32>) -> bool {
    let mut seen = Vec::new();
    for id in order {
        if seen.contains(id) {
            return false;
        }
        for dependency in deps(*id) {
            if order.contains(&dependency) && !seen.contains(&dependency) {
                return false;
            }
        }
        seen.push(*id);
    }
    let mut reversed = order.to_vec();
    reversed.reverse();
    reversed == fini
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
