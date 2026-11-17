// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mkstemp` template handling: the trailing `XXXXXX` check, the unique-name characters and the
//! in-place substitution. The shell owns the `open`/`O_EXCL` loop and the entropy source.

/// The number of trailing `X` characters a `mkstemp` template carries.
pub const SUFFIX_LEN: usize = 6;

/// The character set used for the six replacement characters.
const ALPHABET: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Whether `template` ends with exactly six `X` characters.
#[must_use]
pub fn is_template(template: &[u8]) -> bool {
    let Some(tail) = template
        .len()
        .checked_sub(SUFFIX_LEN)
        .and_then(|at| template.get(at..))
    else {
        return false;
    };
    tail.len() == SUFFIX_LEN && tail.iter().all(|byte| *byte == b'X')
}

/// Map six entropy bytes to six portable file-name characters.
pub fn encode_suffix(raw: &[u8; SUFFIX_LEN], out: &mut [u8; SUFFIX_LEN]) {
    for (slot, byte) in out.iter_mut().zip(raw.iter()) {
        // Mask, not `%`: the crate denies `arithmetic_side_effects`; indices 62/63 fall back.
        let index = usize::from(*byte) & 0x3f;
        *slot = ALPHABET.get(index).copied().unwrap_or(b'X');
    }
}

/// Replace the trailing six `X` characters with `suffix`.
///
/// Returns `false` when `template` is not a valid template, and leaves it unchanged.
#[must_use]
pub fn substitute(template: &mut [u8], suffix: &[u8; SUFFIX_LEN]) -> bool {
    if !is_template(template) {
        return false;
    }
    let Some(at) = template.len().checked_sub(SUFFIX_LEN) else {
        return false;
    };
    let Some(tail) = template.get_mut(at..) else {
        return false;
    };
    for (slot, byte) in tail.iter_mut().zip(suffix.iter()) {
        *slot = *byte;
    }
    true
}

#[cfg(test)]
#[path = "mkstemp_ut.rs"]
mod tests;
