// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the wide-string algorithms from musl:src/string/wcs*.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941, plus the OpenBSD wcslcpy/wcslcat
// (ISC; Todd C. Miller) @ 4ce71f2817f675745b0596aaf04de4a122b1cc52.
// Slices never contain the NUL terminator.

//! Wide-string operations on `u32` code units.

/// `wcslen`.
#[must_use]
pub fn wcslen(s: &[u32]) -> usize {
    s.iter().position(|unit| *unit == 0).unwrap_or(s.len())
}

/// `wcscmp`.
#[must_use]
pub fn wcscmp(a: &[u32], b: &[u32]) -> i32 {
    wcsncmp(a, b, usize::MAX)
}

/// `wcsncmp`.
#[must_use]
pub fn wcsncmp(a: &[u32], b: &[u32], n: usize) -> i32 {
    for index in 0..n {
        let left = a.get(index).copied().unwrap_or(0);
        let right = b.get(index).copied().unwrap_or(0);
        if left != right {
            return if left < right { -1 } else { 1 };
        }
        if left == 0 {
            return 0;
        }
    }
    0
}

/// `wcschr`: return the first index of `unit`.
#[must_use]
pub fn wcschr(s: &[u32], unit: u32) -> Option<usize> {
    s.iter().position(|value| *value == unit)
}

/// `wcsrchr`: return the last index of `unit`.
#[must_use]
pub fn wcsrchr(s: &[u32], unit: u32) -> Option<usize> {
    s.iter().rposition(|value| *value == unit)
}

/// `wcscspn`: length of the initial segment not in `reject`.
#[must_use]
pub fn wcscspn(s: &[u32], reject: &[u32]) -> usize {
    s.iter()
        .position(|value| reject.contains(value))
        .unwrap_or(s.len())
}

/// `wcsspn`: length of the initial segment in `accept`.
#[must_use]
pub fn wcsspn(s: &[u32], accept: &[u32]) -> usize {
    s.iter()
        .position(|value| !accept.contains(value))
        .unwrap_or(s.len())
}

/// `wcspbrk`: first index in `accept`.
#[must_use]
pub fn wcspbrk(s: &[u32], accept: &[u32]) -> Option<usize> {
    s.iter().position(|value| accept.contains(value))
}

/// `wcsstr`: first index of `needle`.
#[must_use]
pub fn wcsstr(haystack: &[u32], needle: &[u32]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Copy `src` into `dst`; return the number of code units copied.
pub fn wcscpy(dst: &mut [u32], src: &[u32]) -> usize {
    let count = src.len().min(dst.len());
    if let (Some(out), Some(from)) = (dst.get_mut(..count), src.get(..count)) {
        out.copy_from_slice(from);
    }
    count
}

/// Copy at most `n` code units; return the number copied.
pub fn wcsncpy(dst: &mut [u32], src: &[u32], n: usize) -> usize {
    let count = n.min(src.len()).min(dst.len());
    if let (Some(out), Some(from)) = (dst.get_mut(..count), src.get(..count)) {
        out.copy_from_slice(from);
    }
    count
}

/// Append `src` to the non-NUL part of `dst`; return the new length.
pub fn wcscat(dst: &mut [u32], src: &[u32]) -> usize {
    let used = wcslen(dst);
    let room = dst.len().saturating_sub(used);
    let count = src.len().min(room);
    if let (Some(out), Some(from)) = (dst.get_mut(used..used + count), src.get(..count)) {
        out.copy_from_slice(from);
    }
    used + count
}

/// Append at most `n` code units; return the new length.
pub fn wcsncat(dst: &mut [u32], src: &[u32], n: usize) -> usize {
    let used = wcslen(dst);
    let room = dst.len().saturating_sub(used);
    let count = n.min(src.len()).min(room);
    if let (Some(out), Some(from)) = (dst.get_mut(used..used + count), src.get(..count)) {
        out.copy_from_slice(from);
    }
    used + count
}

/// `wcslcpy`: copy, always NUL-terminating when `size > 0`; returns `src.len()`.
pub fn wcslcpy(dst: &mut [u32], src: &[u32]) -> usize {
    if let Some(last) = dst.len().checked_sub(1) {
        let count = src.len().min(last);
        if let (Some(out), Some(from)) = (dst.get_mut(..count), src.get(..count)) {
            out.copy_from_slice(from);
        }
        if let Some(terminator) = dst.get_mut(count) {
            *terminator = 0;
        }
    }
    src.len()
}

/// `wcslcat`: append, always NUL-terminating when `size > 0`; returns the would-be length.
pub fn wcslcat(dst: &mut [u32], src: &[u32]) -> usize {
    let used = wcslen(dst);
    let total = used.saturating_add(src.len());
    if let Some(last) = dst.len().checked_sub(1) {
        let count = src.len().min(last.saturating_sub(used));
        if let (Some(out), Some(from)) = (dst.get_mut(used..used + count), src.get(..count)) {
            out.copy_from_slice(from);
        }
        let end = used.saturating_add(count).min(last);
        if let Some(terminator) = dst.get_mut(end) {
            *terminator = 0;
        }
    }
    total
}

/// `wmemchr`.
#[must_use]
pub fn wmemchr(s: &[u32], unit: u32) -> Option<usize> {
    s.iter().position(|value| *value == unit)
}

/// `wmemcmp`: unsigned code-unit order.
#[must_use]
pub fn wmemcmp(a: &[u32], b: &[u32], n: usize) -> i32 {
    for index in 0..n {
        let left = a.get(index).copied().unwrap_or(0);
        let right = b.get(index).copied().unwrap_or(0);
        if left != right {
            return if left < right { -1 } else { 1 };
        }
    }
    0
}

/// `wmemcpy`; returns the number copied.
pub fn wmemcpy(dst: &mut [u32], src: &[u32]) -> usize {
    let count = src.len().min(dst.len());
    if let (Some(out), Some(from)) = (dst.get_mut(..count), src.get(..count)) {
        out.copy_from_slice(from);
    }
    count
}

/// `wmemmove`; overlapping-safe copy.
pub fn wmemmove(dst: &mut [u32], src: &[u32]) -> usize {
    let count = src.len().min(dst.len());
    if let Some(from) = src.get(..count)
        && let Some(out) = dst.get_mut(..count)
    {
        if out.as_ptr() <= from.as_ptr() {
            out.copy_from_slice(from);
        } else {
            for index in (0..count).rev() {
                if let (Some(target), Some(value)) = (out.get_mut(index), from.get(index)) {
                    *target = *value;
                }
            }
        }
    }
    count
}

/// `wmemset`; returns `n`.
pub fn wmemset(dst: &mut [u32], unit: u32, n: usize) -> usize {
    let count = n.min(dst.len());
    if let Some(out) = dst.get_mut(..count) {
        out.fill(unit);
    }
    count
}

/// `wcstok`: split `s` at the units in `delim`, using `cursor`.
pub fn wcstok<'a>(s: &'a [u32], delim: &[u32], cursor: &mut &'a [u32]) -> Option<&'a [u32]> {
    let mut rest = if s.is_empty() { *cursor } else { s };
    let start = rest.iter().position(|unit| !delim.contains(unit))?;
    rest = rest.get(start..)?;
    let end = rest
        .iter()
        .position(|unit| delim.contains(unit))
        .unwrap_or(rest.len());
    let token = rest.get(..end)?;
    *cursor = rest.get(end..).unwrap_or(&[]);
    Some(token)
}

#[cfg(test)]
#[path = "wstring_ut.rs"]
mod tests;
