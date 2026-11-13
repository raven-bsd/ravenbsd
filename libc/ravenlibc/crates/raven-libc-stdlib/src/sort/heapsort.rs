// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1991, 1993 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/heapsort.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: heapsort.c,v 1.12 2017/01/24 08:09:12 tb Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! `heapsort`: the public BSD extension, with its donor key buffer.
//!
//! The key buffer is a `Vec<u8>` of one element; [`SortError::NoMem`] is the donor's failed
//! `malloc` (`heapsort.c:149`), and `nmemb * size` is checked (`EINVAL`, A-stdlib-1).

#![allow(clippy::arithmetic_side_effects)] // RavenBSD: donor index arithmetic, bounded by base.len()

use super::SortError;

fn elem(base: &[u8], es: usize, index: usize) -> &[u8] {
    let start = index.wrapping_mul(es);
    base.get(start..start.wrapping_add(es)).unwrap_or(&[])
}

fn cmp_at<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &[u8],
    es: usize,
    i: usize,
    j: usize,
    cmp: &mut F,
) -> i32 {
    cmp(elem(base, es, i), elem(base, es, j))
}

fn swap_elems(base: &mut [u8], es: usize, i: usize, j: usize) {
    if i == j || es == 0 {
        return;
    }
    let (low, high) = if i < j { (i, j) } else { (j, i) };
    let low_start = low.wrapping_mul(es);
    let high_start = high.wrapping_mul(es);
    let low_end = low_start.wrapping_add(es);
    if high_start < low_end || high_start.wrapping_add(es) > base.len() {
        return;
    }
    let (left, right) = base.split_at_mut(high_start);
    if let (Some(a), Some(b)) = (left.get_mut(low_start..low_end), right.get_mut(..es)) {
        a.swap_with_slice(b);
    }
}

/// Copy `es` bytes from `src[index]` to `dst[index]`; the buffers are distinct.
fn copy_to(dst: &mut [u8], src: &[u8], es: usize, dst_index: usize, src_index: usize) {
    let from = src_index.wrapping_mul(es);
    let to = dst_index.wrapping_mul(es);
    if let (Some(source), Some(target)) = (
        src.get(from..from.wrapping_add(es)),
        dst.get_mut(to..to.wrapping_add(es)),
    ) {
        target.copy_from_slice(source);
    }
}

/// Copy one element inside one buffer (`COPY(par, child)`).
fn copy_within(base: &mut [u8], es: usize, dst_index: usize, src_index: usize) {
    let from = src_index.wrapping_mul(es);
    let to = dst_index.wrapping_mul(es);
    if from.wrapping_add(es) <= base.len() && to.wrapping_add(es) <= base.len() {
        base.copy_within(from..from.wrapping_add(es), to);
    }
}

/// `CREATE`: sift the element at 1-based `initval` down with swaps.
fn create<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    es: usize,
    initval: usize,
    nmemb: usize,
    cmp: &mut F,
) {
    let mut par_i = initval;
    loop {
        let mut child_i = par_i.wrapping_mul(2);
        if child_i > nmemb {
            break;
        }
        if child_i < nmemb && cmp_at(base, es, child_i - 1, child_i, cmp) < 0 {
            child_i = child_i.wrapping_add(1);
        }
        if cmp_at(base, es, child_i - 1, par_i - 1, cmp) <= 0 {
            break;
        }
        swap_elems(base, es, par_i - 1, child_i - 1);
        par_i = child_i;
    }
}

/// `SELECT`: copy the larger child up, then bubble the displaced key `k` up from the bottom.
fn select<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    es: usize,
    nmemb: usize,
    key: &[u8],
    cmp: &mut F,
) {
    let mut par_i = 1usize;
    loop {
        let mut child_i = par_i.wrapping_mul(2);
        if child_i > nmemb {
            break;
        }
        if child_i < nmemb && cmp_at(base, es, child_i - 1, child_i, cmp) < 0 {
            child_i = child_i.wrapping_add(1);
        }
        // COPY(par, child)
        copy_within(base, es, par_i - 1, child_i - 1);
        par_i = child_i;
    }
    let mut child_i = par_i;
    loop {
        let parent_i = child_i / 2;
        if child_i == 1 || cmp(key, elem(base, es, parent_i - 1)) < 0 {
            copy_to(base, key, es, child_i - 1, 0);
            break;
        }
        copy_within(base, es, child_i - 1, parent_i - 1);
        child_i = parent_i;
    }
}

/// `heapsort(base, nmemb, size, compar)`; `base.len()` must be `nmemb * size`.
///
/// # Errors
///
/// [`SortError::Inval`] when `size == 0` and `nmemb > 1`, or when `nmemb * size` overflows;
/// [`SortError::NoMem`] when the key buffer cannot be allocated.
pub fn heapsort<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    es: usize,
    mut cmp: F,
) -> Result<(), SortError> {
    if es == 0 {
        if base.is_empty() {
            return Ok(());
        }
        return Err(SortError::Inval);
    }
    let Some(mut nmemb) = base.len().checked_div(es) else {
        return Err(SortError::Inval);
    };
    if nmemb <= 1 {
        return Ok(());
    }
    let total = nmemb.checked_mul(es).ok_or(SortError::Inval)?;
    if total > base.len() {
        return Err(SortError::Inval);
    }
    let mut key: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    key.try_reserve_exact(es).map_err(|_| SortError::NoMem)?;
    key.resize(es, 0);

    let mut l = nmemb / 2;
    while l > 0 {
        create(base, es, l, nmemb, &mut cmp);
        l = l.wrapping_sub(1);
    }

    while nmemb > 1 {
        // COPY(k, base + nmemb * size)
        copy_to(&mut key, base, es, 0, nmemb - 1);
        // COPY(base + nmemb * size, base + size)
        copy_within(base, es, nmemb - 1, 0);
        nmemb = nmemb.wrapping_sub(1);
        select(base, es, nmemb, &key, &mut cmp);
    }
    Ok(())
}

#[cfg(test)]
#[path = "heapsort_ut.rs"]
mod tests;
