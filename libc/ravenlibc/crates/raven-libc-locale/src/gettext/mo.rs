// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the `.mo` lookup from musl:src/locale/__mo_lookup.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941.

//! GNU `.mo` catalog lookup.

fn read_u32(mo: &[u8], offset: usize, big: bool) -> Option<u32> {
    let bytes = mo.get(offset..offset.checked_add(4)?)?;
    let array = [
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
    ];
    Some(if big {
        u32::from_be_bytes(array)
    } else {
        u32::from_le_bytes(array)
    })
}

fn endian(mo: &[u8]) -> Option<bool> {
    let magic = mo.get(..4)?;
    let array = [
        *magic.first()?,
        *magic.get(1)?,
        *magic.get(2)?,
        *magic.get(3)?,
    ];
    if u32::from_le_bytes(array) == 0x9504_12de {
        Some(false)
    } else if u32::from_be_bytes(array) == 0x9504_12de {
        Some(true)
    } else {
        None
    }
}

/// Find `msgid` in a `.mo` image.
#[must_use]
pub fn mo_lookup<'a>(mo: &'a [u8], msgid: &[u8]) -> Option<&'a [u8]> {
    let big = endian(mo)?;
    let count = read_u32(mo, 8, big)? as usize;
    let originals = read_u32(mo, 12, big)? as usize;
    let translations = read_u32(mo, 16, big)? as usize;
    let size = mo.len();
    if count >= size / 4
        || originals >= size.saturating_sub(4 * count)
        || translations >= size.saturating_sub(4 * count)
        || !(originals | translations).is_multiple_of(4)
    {
        return None;
    }
    let mut low = 0usize;
    let mut high = count;
    while low < high {
        let middle = low.checked_add(high.saturating_sub(low) / 2)?;
        let original_length = read_u32(mo, originals.checked_add(8 * middle)?, big)? as usize;
        let original_offset =
            read_u32(mo, originals.checked_add(8 * middle)?.checked_add(4)?, big)? as usize;
        if original_offset >= size || original_length >= size.saturating_sub(original_offset) {
            return None;
        }
        if mo.get(original_offset.checked_add(original_length)?) != Some(&0) {
            return None;
        }
        let candidate = mo.get(original_offset..original_offset.checked_add(original_length)?)?;
        match candidate.cmp(msgid) {
            core::cmp::Ordering::Equal => {
                let translation_length =
                    read_u32(mo, translations.checked_add(8 * middle)?, big)? as usize;
                let translation_offset = read_u32(
                    mo,
                    translations.checked_add(8 * middle)?.checked_add(4)?,
                    big,
                )? as usize;
                if translation_offset >= size
                    || translation_length >= size.saturating_sub(translation_offset)
                {
                    return None;
                }
                if mo.get(translation_offset.checked_add(translation_length)?) != Some(&0) {
                    return None;
                }
                return mo
                    .get(translation_offset..translation_offset.checked_add(translation_length)?);
            }
            core::cmp::Ordering::Less => low = middle.checked_add(1)?,
            core::cmp::Ordering::Greater => high = middle,
        }
    }
    None
}

#[cfg(test)]
#[path = "mo_ut.rs"]
mod tests;
