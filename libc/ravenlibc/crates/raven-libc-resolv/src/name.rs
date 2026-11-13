// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! DNS name decompression, compression, skipping and the hostname sanity check.

use alloc::vec::Vec;

use crate::consts::{DNS_LABEL_MAX, DNS_NAME_MAX};

/// `is_valid_hostname` from `lookup_name.c`: 1..=254 bytes, valid UTF-8, and only alphanumerics,
/// `-`, `.` or bytes `>= 0x80`. Applied to CNAME and PTR targets (D-12-7).
#[must_use]
pub fn is_valid_hostname(host: &str) -> bool {
    let bytes = host.as_bytes();
    if bytes.is_empty() || bytes.len() > 254 {
        return false;
    }
    if core::str::from_utf8(bytes).is_err() {
        return false;
    }
    bytes
        .iter()
        .all(|byte| *byte >= 0x80 || *byte == b'.' || *byte == b'-' || byte.is_ascii_alphanumeric())
}

/// `__dn_expand`: decompress the name at `pos`.
///
/// Returns `(bytes consumed in the message, text length)`. The text is NUL-terminated in `out`;
/// at most 254 text bytes are produced. The pointer loop is guarded by an iteration counter,
/// never recursion.
pub fn dn_expand(msg: &[u8], pos: usize, out: &mut [u8; 256]) -> Option<(usize, usize)> {
    if pos >= msg.len() {
        return None;
    }
    let mut p = pos;
    let mut dest = 0usize;
    let mut consumed: Option<usize> = None;
    let mut iteration = 0usize;
    while iteration < msg.len() {
        iteration = iteration.saturating_add(2);
        let byte = *msg.get(p)?;
        if byte & 0xc0 != 0 {
            if p.checked_add(1)? == msg.len() {
                return None;
            }
            let target =
                (usize::from(byte & 0x3f) << 8) | usize::from(msg.get(p.checked_add(1)?).copied()?);
            if consumed.is_none() {
                consumed = Some(p.checked_add(2)?.checked_sub(pos)?);
            }
            if target >= msg.len() {
                return None;
            }
            p = target;
        } else if byte != 0 {
            if dest != 0 {
                if dest >= 254 {
                    return None;
                }
                if let Some(slot) = out.get_mut(dest) {
                    *slot = b'.';
                }
                dest = dest.saturating_add(1);
            }
            p = p.checked_add(1)?;
            let label = usize::from(byte);
            if label >= msg.len().saturating_sub(p) || label >= 254usize.saturating_sub(dest) {
                return None;
            }
            let source = msg.get(p..p.checked_add(label)?)?;
            let target = out.get_mut(dest..dest.checked_add(label)?)?;
            target.copy_from_slice(source);
            dest = dest.saturating_add(label);
            p = p.saturating_add(label);
        } else {
            if let Some(slot) = out.get_mut(dest) {
                *slot = 0;
            }
            let consumed = match consumed {
                Some(value) => value,
                None => p.checked_add(1)?.checked_sub(pos)?,
            };
            return Some((consumed, dest));
        }
    }
    None
}

/// Collect the label offsets of the compressed name starting at `offset`.
fn name_offsets(msg: &[u8], offset: usize) -> Option<Vec<usize>> {
    let mut offsets: Vec<usize> = Vec::new();
    let mut p = offset;
    let mut guard = 0usize;
    while guard <= msg.len() {
        guard = guard.saturating_add(1);
        let byte = *msg.get(p)?;
        if byte & 0xc0 == 0xc0 {
            if p.checked_add(1)? >= msg.len() {
                return None;
            }
            let target =
                (usize::from(byte & 0x3f) << 8) | usize::from(msg.get(p.checked_add(1)?).copied()?);
            if target >= msg.len() || target >= 0x4000 {
                return None;
            }
            p = target;
            continue;
        }
        if byte == 0 {
            return Some(offsets);
        }
        if usize::from(byte) > DNS_LABEL_MAX {
            return None;
        }
        if offsets.len() >= 127 || p >= 0x4000 {
            return None;
        }
        offsets.push(p);
        p = p.checked_add(usize::from(byte))?.checked_add(1)?;
    }
    None
}

/// Split a text name (without the trailing dot) into label lengths.
fn text_labels(name: &[u8]) -> Option<Vec<usize>> {
    let mut lengths: Vec<usize> = Vec::new();
    let mut start = 0usize;
    loop {
        let mut end = start;
        while name.get(end).copied().is_some_and(|byte| byte != b'.') {
            end = end.saturating_add(1);
        }
        let length = end.checked_sub(start)?;
        if length == 0 || length > DNS_LABEL_MAX || lengths.len() >= 127 {
            return None;
        }
        lengths.push(length);
        if end == name.len() {
            return Some(lengths);
        }
        start = end.checked_add(1)?;
    }
}

/// The byte position of every label in the text `name`.
fn label_positions(name: &[u8], labels: &[usize]) -> Option<Vec<usize>> {
    let mut positions: Vec<usize> = Vec::with_capacity(labels.len());
    let mut position = 0usize;
    for length in labels.iter().copied() {
        positions.push(position);
        position = position.checked_add(length)?.checked_add(1)?;
    }
    if positions.len() != labels.len() || position > name.len().saturating_add(1) {
        return None;
    }
    Some(positions)
}

/// The length in text bytes of the longest suffix of `name` that matches the compressed name
/// at `offset`, plus the offset of that suffix's first label.
///
/// The matched length includes the dot separating the unmatched prefix from the suffix (the
/// donor's `m` accounting), so the unmatched byte count is `name.len() - matched`.
fn suffix_match(
    msg: &[u8],
    name: &[u8],
    labels: &[usize],
    positions: &[usize],
    offset: usize,
) -> Option<(usize, usize)> {
    let offsets = name_offsets(msg, offset)?;
    let mut matched = 0usize;
    let mut name_index = labels.len();
    let mut offset_index = offsets.len();
    let mut point_at = offset;
    loop {
        if name_index == 0 || offset_index == 0 {
            return Some((matched, point_at));
        }
        name_index = name_index.saturating_sub(1);
        offset_index = offset_index.saturating_sub(1);
        let name_length = *labels.get(name_index)?;
        let name_start = *positions.get(name_index)?;
        let node = *offsets.get(offset_index)?;
        let compressed_length = usize::from(*msg.get(node)?);
        if compressed_length != name_length {
            return Some((matched, point_at));
        }
        let name_bytes = name.get(name_start..name_start.checked_add(name_length)?)?;
        let node_start = node.checked_add(1)?;
        let msg_bytes = msg.get(node_start..node_start.checked_add(compressed_length)?)?;
        if name_bytes != msg_bytes {
            return Some((matched, point_at));
        }
        point_at = node;
        matched = matched.saturating_add(name_length);
        if name_index > 0 {
            matched = matched.saturating_add(1);
        }
        if name_index == 0 || offset_index == 0 {
            return Some((matched, point_at));
        }
    }
}

/// `dn_comp`: compress `name` into the message buffer `dst`.
///
/// `ptrs` is the compression table: `ptrs[0]` is the offset in `dst` where this name is
/// written, entries `1..last` are offsets of earlier names, and a successfully written name is
/// appended at index `last` (when the caller supplied one). Returns the encoded byte length.
pub fn dn_comp(
    name: &[u8],
    dst: &mut [u8],
    ptrs: &mut [usize],
    last: Option<usize>,
) -> Option<usize> {
    let mut length = name.len().min(255);
    if length > 0 && name.get(length.saturating_sub(1)).copied() == Some(b'.') {
        length = length.saturating_sub(1);
    }
    if length > 0 && name.get(length.saturating_sub(1)).copied() == Some(b'.') {
        return None;
    }
    if length > DNS_NAME_MAX || dst.is_empty() {
        return None;
    }
    let base = ptrs.first().copied().unwrap_or(0);
    if base >= dst.len() {
        return None;
    }
    if length == 0 {
        if let Some(slot) = dst.get_mut(base) {
            *slot = 0;
        }
        return Some(1);
    }
    let text = name.get(..length)?;
    let labels = text_labels(text)?;
    let positions = label_positions(text, &labels)?;

    let table_len = match last {
        Some(value) => value.min(ptrs.len()),
        None => 0,
    };
    let mut best_length = 0usize;
    let mut best_offset = 0usize;
    if table_len > 1 {
        for index in 1..table_len {
            let Some(offset) = ptrs.get(index).copied() else {
                continue;
            };
            if offset >= dst.len() {
                continue;
            }
            let Some((matched, at)) = suffix_match(dst, text, &labels, &positions, offset) else {
                continue;
            };
            if matched > best_length {
                best_length = matched;
                best_offset = at;
                if matched == length {
                    break;
                }
            }
        }
    }

    let mut encoded: Vec<u8> = Vec::with_capacity(length.saturating_add(3));
    let unmatched = length.saturating_sub(best_length);
    let mut consumed = 0usize;
    let mut label_index = 0usize;
    while consumed < unmatched {
        let label_length = *labels.get(label_index)?;
        encoded.push(u8::try_from(label_length).ok()?);
        let start = *positions.get(label_index)?;
        let end = start.checked_add(label_length)?;
        encoded.extend_from_slice(text.get(start..end)?);
        label_index = label_index.saturating_add(1);
        consumed = consumed.checked_add(label_length)?.checked_add(1)?;
    }
    if best_length > 0 {
        encoded.push(u8::try_from(best_offset >> 8).ok()? | 0xc0);
        encoded.push(u8::try_from(best_offset & 0xff).ok()?);
    } else {
        encoded.push(0);
    }
    let target = dst.get_mut(base..base.checked_add(encoded.len())?)?;
    target.copy_from_slice(&encoded);
    if encoded.len() > 2
        && let Some(table_index) = last
        && table_index > 0
        && table_index < ptrs.len()
        && let Some(slot) = ptrs.get_mut(table_index)
    {
        *slot = base;
    }
    Some(encoded.len())
}

/// `dn_skipname`: the number of bytes the name occupies, or `None` when malformed.
#[must_use]
pub fn dn_skipname(msg: &[u8], pos: usize) -> Option<usize> {
    let mut p = pos;
    while p < msg.len() {
        let byte = *msg.get(p)?;
        if byte == 0 {
            return p.checked_sub(pos)?.checked_add(1);
        }
        if byte >= 192 {
            if p.checked_add(1)? < msg.len() {
                return p.checked_sub(pos)?.checked_add(2);
            }
            return None;
        }
        let needed = usize::from(byte).checked_add(1)?;
        if msg.len().saturating_sub(p) < needed {
            return None;
        }
        p = p.checked_add(needed)?;
    }
    None
}

#[cfg(test)]
#[path = "name_ut.rs"]
mod tests;
