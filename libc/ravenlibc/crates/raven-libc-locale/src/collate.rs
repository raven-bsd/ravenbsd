// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: code-point collation from musl:src/locale/{strcoll,strxfrm,wcscoll,wcsxfrm}.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941.

//! Code-point-order collation.

/// `strcoll`: unsigned byte order.
#[must_use]
pub fn strcoll(a: &[u8], b: &[u8]) -> i32 {
    cmp_bytes(a, b)
}

fn cmp_bytes(a: &[u8], b: &[u8]) -> i32 {
    let len = a.len().min(b.len());
    for index in 0..len {
        let (Some(left), Some(right)) = (a.get(index), b.get(index)) else {
            break;
        };
        if left != right {
            return i32::from(*left) - i32::from(*right);
        }
    }
    match a.len().cmp(&b.len()) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

/// `strxfrm`: return the source length; copy only when `dst` is longer than the source.
pub fn strxfrm(dst: Option<&mut [u8]>, src: &[u8]) -> usize {
    if let Some(dst) = dst
        && dst.len() > src.len()
        && let Some(out) = dst.get_mut(..src.len())
    {
        out.copy_from_slice(src);
    }
    src.len()
}

/// `wcscoll`: unsigned code-unit order.
#[must_use]
pub fn wcscoll(a: &[u32], b: &[u32]) -> i32 {
    let len = a.len().min(b.len());
    for index in 0..len {
        let (Some(left), Some(right)) = (a.get(index), b.get(index)) else {
            break;
        };
        if left != right {
            return if *left < *right { -1 } else { 1 };
        }
    }
    match a.len().cmp(&b.len()) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

/// `wcsxfrm`: return the source length; truncate and NUL-terminate when `dst` is non-empty.
pub fn wcsxfrm(dst: Option<&mut [u32]>, src: &[u32]) -> usize {
    if let Some(dst) = dst
        && let Some(last) = dst.len().checked_sub(1)
    {
        let copy = src.len().min(last);
        if let Some(out) = dst.get_mut(..copy)
            && let Some(from) = src.get(..copy)
        {
            out.copy_from_slice(from);
        }
        if let Some(terminator) = dst.get_mut(copy) {
            *terminator = 0;
        }
    }
    src.len()
}

#[cfg(test)]
#[path = "collate_ut.rs"]
mod tests;
