// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/memmem.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: memmem.c,v 1.6 2026/06/23 13:09:11 tim Exp $)
// Copyright (c) 2005-2020 Rich Felker, et al. (the donor header names the same terms)
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! `memmem` (Two-Way string matching) over slices.
//!
//! RavenBSD: the Two-Way index arithmetic is bounded by the haystack and needle lengths; the
//! lint allow records that proof instead of scattering `wrapping_*` over the donor's branch
//! structure.

#![allow(clippy::arithmetic_side_effects)]

use super::find;

const SET_WORDS: usize = 32 / core::mem::size_of::<usize>();

#[inline]
fn byte_at(bytes: &[u8], index: isize) -> u8 {
    if index < 0 {
        return 0;
    }
    bytes.get(index as usize).copied().unwrap_or(0)
}

#[inline]
fn equal_at(left: &[u8], lo: usize, right: &[u8], ro: usize, len: usize) -> bool {
    let mut index = 0usize;
    while index < len {
        if byte_at(left, (lo + index) as isize) != byte_at(right, (ro + index) as isize) {
            return false;
        }
        index += 1;
    }
    true
}

#[inline]
fn bitop(set: &mut [usize; SET_WORDS], byte: u8, or: bool) {
    let slot = usize::from(byte) / (8 * core::mem::size_of::<usize>());
    let bit = usize::from(byte) % (8 * core::mem::size_of::<usize>());
    if let Some(word) = set.get_mut(slot) {
        let mask = 1usize << bit;
        if or {
            *word |= mask;
        } else {
            *word &= mask;
        }
    }
}

#[inline]
fn bit_test(set: &[usize; SET_WORDS], byte: u8) -> bool {
    let slot = usize::from(byte) / (8 * core::mem::size_of::<usize>());
    let bit = usize::from(byte) % (8 * core::mem::size_of::<usize>());
    match set.get(slot) {
        Some(word) => (word & (1usize << bit)) != 0,
        None => false,
    }
}

fn twobyte_memmem(h: &[u8], k: usize, n: &[u8]) -> Option<usize> {
    let nw = (u16::from(byte_at(n, 0)) << 8) | u16::from(byte_at(n, 1));
    let mut hw = (u16::from(byte_at(h, 0)) << 8) | u16::from(byte_at(h, 1));
    let mut index = 0usize;
    // C: for (h+=2, k-=2; k; k--, hw = hw<<8 | *h++)
    if k < 2 {
        return if hw == nw { Some(0) } else { None };
    }
    let mut remaining = k - 2;
    index += 2;
    while remaining != 0 {
        if hw == nw {
            return Some(index - 2);
        }
        hw = (hw << 8) | u16::from(byte_at(h, index as isize));
        index += 1;
        remaining -= 1;
    }
    if hw == nw { Some(index - 2) } else { None }
}

fn threebyte_memmem(h: &[u8], k: usize, n: &[u8]) -> Option<usize> {
    let nw = (u32::from(byte_at(n, 0)) << 24)
        | (u32::from(byte_at(n, 1)) << 16)
        | (u32::from(byte_at(n, 2)) << 8);
    let mut hw = (u32::from(byte_at(h, 0)) << 24)
        | (u32::from(byte_at(h, 1)) << 16)
        | (u32::from(byte_at(h, 2)) << 8);
    if k < 3 {
        return if hw == nw { Some(0) } else { None };
    }
    let mut index = 3usize;
    let mut remaining = k - 3;
    while remaining != 0 {
        if hw == nw {
            return Some(index - 3);
        }
        hw = (hw | u32::from(byte_at(h, index as isize))).wrapping_shl(8);
        index += 1;
        remaining -= 1;
    }
    if hw == nw { Some(index - 3) } else { None }
}

fn fourbyte_memmem(h: &[u8], k: usize, n: &[u8]) -> Option<usize> {
    let nw = (u32::from(byte_at(n, 0)) << 24)
        | (u32::from(byte_at(n, 1)) << 16)
        | (u32::from(byte_at(n, 2)) << 8)
        | u32::from(byte_at(n, 3));
    let mut hw = (u32::from(byte_at(h, 0)) << 24)
        | (u32::from(byte_at(h, 1)) << 16)
        | (u32::from(byte_at(h, 2)) << 8)
        | u32::from(byte_at(h, 3));
    if k < 4 {
        return if hw == nw { Some(0) } else { None };
    }
    let mut index = 4usize;
    let mut remaining = k - 4;
    while remaining != 0 {
        if hw == nw {
            return Some(index - 4);
        }
        hw = (hw << 8) | u32::from(byte_at(h, index as isize));
        index += 1;
        remaining -= 1;
    }
    if hw == nw { Some(index - 4) } else { None }
}

fn twoway_memmem(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let l = needle.len();
    let mut byteset = [0usize; SET_WORDS];
    let mut shift = [0usize; 256];
    // RavenBSD: the array is initialised in place; it is never moved as a 2 KiB value.

    let mut i = 0usize;
    while i < l {
        let byte = byte_at(needle, i as isize);
        bitop(&mut byteset, byte, true);
        if let Some(slot) = shift.get_mut(usize::from(byte)) {
            *slot = i + 1;
        }
        i += 1;
    }

    // Compute maximal suffix (greater than).
    let mut ip: isize = -1;
    let mut jp: isize = 0;
    let mut k: isize = 1;
    let mut p: isize = 1;
    while jp + k < l as isize {
        if byte_at(needle, ip + k) == byte_at(needle, jp + k) {
            if k == p {
                jp += p;
                k = 1;
            } else {
                k += 1;
            }
        } else if byte_at(needle, ip + k) > byte_at(needle, jp + k) {
            jp += k;
            k = 1;
            p = jp - ip;
        } else {
            ip = jp;
            jp += 1;
            k = 1;
            p = 1;
        }
    }
    let mut ms = ip;
    let p0 = p;

    // And with the opposite comparison.
    ip = -1;
    jp = 0;
    k = 1;
    p = 1;
    while jp + k < l as isize {
        if byte_at(needle, ip + k) == byte_at(needle, jp + k) {
            if k == p {
                jp += p;
                k = 1;
            } else {
                k += 1;
            }
        } else if byte_at(needle, ip + k) < byte_at(needle, jp + k) {
            jp += k;
            k = 1;
            p = jp - ip;
        } else {
            ip = jp;
            jp += 1;
            k = 1;
            p = 1;
        }
    }
    if ip + 1 > ms + 1 {
        ms = ip;
    } else {
        p = p0;
    }

    // Periodic needle?
    let mem0;
    if !equal_at(needle, 0, needle, p as usize, (ms + 1) as usize) {
        mem0 = 0;
        p = core::cmp::max(ms, l as isize - ms - 1) + 1;
    } else {
        mem0 = l as isize - p;
    }
    let mut mem: isize = 0;
    let mut h: usize = 0;

    loop {
        if (haystack.len() as isize - h as isize) < l as isize {
            return None;
        }
        let last = byte_at(haystack, (h + l - 1) as isize);
        if bit_test(&byteset, last) {
            k = l as isize - shift.get(usize::from(last)).copied().unwrap_or(0) as isize;
            if k != 0 {
                if k < mem {
                    k = mem;
                }
                h += k as usize;
                mem = 0;
                continue;
            }
        } else {
            h += l;
            mem = 0;
            continue;
        }

        // Compare right half.
        k = core::cmp::max(ms + 1, mem);
        while k < l as isize && byte_at(needle, k) == byte_at(haystack, h as isize + k) {
            k += 1;
        }
        if k < l as isize {
            h += (k - ms) as usize;
            mem = 0;
            continue;
        }
        // Compare left half.
        k = ms + 1;
        while k > mem && byte_at(needle, k - 1) == byte_at(haystack, h as isize + k - 1) {
            k -= 1;
        }
        if k <= mem {
            return Some(h);
        }
        h += p as usize;
        mem = mem0;
    }
}

/// Index of the first occurrence of `needle` in `haystack` (`memmem`).
///
/// An empty needle matches at offset 0, a needle longer than the haystack does not match.
#[must_use]
pub fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let l = needle.len();
    if l == 0 {
        return Some(0);
    }
    if haystack.len() < l {
        return None;
    }
    let first = byte_at(needle, 0);
    let offset = find(haystack, first)?;
    if l == 1 {
        return Some(offset);
    }
    let h = haystack.get(offset..)?;
    if h.len() < l {
        return None;
    }
    let found = match l {
        2 => twobyte_memmem(h, h.len(), needle),
        3 => threebyte_memmem(h, h.len(), needle),
        4 => fourbyte_memmem(h, h.len(), needle),
        _ => twoway_memmem(h, needle),
    };
    found.map(|index| offset + index)
}

#[cfg(test)]
#[path = "memmem_ut.rs"]
mod tests;
