// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1992, 1993 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/merge.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: merge.c,v 1.13 2017/01/24 08:09:12 tb Exp $)
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

//! `mergesort`: the stable BSD extension.
//!
//! RavenBSD: the donor's in-buffer linked list (`setup`/`EVAL`, `merge.c:82-107,222-320`) is
//! replaced by an index-based stable natural merge with the donor's insertion-sort threshold
//! (`THRESHOLD 16`) and its `<=` left-run preference; the output of a stable sort is unique, and
//! the ticket tests stability rather than the donor's link layout. `merge.c:115`'s unchecked
//! `malloc(nmemb * size + PSIZE)` becomes `checked_mul`/`checked_add` + `try_reserve`
//! (A-stdlib-1).

#![allow(clippy::arithmetic_side_effects)] // RavenBSD: donor index arithmetic, bounded by base.len()

use super::{PSIZE, SortError};

const THRESHOLD: usize = 16;

fn elem(base: &[u8], es: usize, index: usize) -> &[u8] {
    let start = index.wrapping_mul(es);
    base.get(start..start.wrapping_add(es)).unwrap_or(&[])
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

fn insertion_sort<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    es: usize,
    lo: usize,
    hi: usize,
    cmp: &mut F,
) {
    let mut ai = lo.wrapping_add(1);
    while ai < hi {
        let mut t = ai;
        while t > lo && cmp(elem(base, es, t.wrapping_sub(1)), elem(base, es, t)) > 0 {
            swap_elems(base, es, t.wrapping_sub(1), t);
            t = t.wrapping_sub(1);
        }
        ai = ai.wrapping_add(1);
    }
}

/// Copy `es` bytes from `src[src_index]` into `dst[dst_index]`.
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

/// One merge pass over `[lo, hi)` with a run boundary at `mid`.
fn merge_run<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    temp: &mut [u8],
    es: usize,
    lo: usize,
    mid: usize,
    hi: usize,
    cmp: &mut F,
) {
    let mut left = lo;
    let mut right = mid;
    let mut out = lo;
    while left < mid && right < hi {
        // RavenBSD: `<=` copies the left run first, which is the donor's stability rule.
        if cmp(elem(base, es, left), elem(base, es, right)) <= 0 {
            copy_to(temp, base, es, out, left);
            left = left.wrapping_add(1);
        } else {
            copy_to(temp, base, es, out, right);
            right = right.wrapping_add(1);
        }
        out = out.wrapping_add(1);
    }
    while left < mid {
        copy_to(temp, base, es, out, left);
        left = left.wrapping_add(1);
        out = out.wrapping_add(1);
    }
    while right < hi {
        copy_to(temp, base, es, out, right);
        right = right.wrapping_add(1);
        out = out.wrapping_add(1);
    }
    let from = lo.wrapping_mul(es);
    let to = hi.wrapping_mul(es);
    if let (Some(target), Some(source)) = (base.get_mut(from..to), temp.get(from..to)) {
        target.copy_from_slice(source);
    }
    let _ = out;
}

/// The donor's `nmemb * size + PSIZE` with both operations checked (A-stdlib-1).
pub(crate) fn checked_capacity(nmemb: usize, es: usize) -> Result<usize, SortError> {
    let total = nmemb.checked_mul(es).ok_or(SortError::NoMem)?;
    total.checked_add(PSIZE).ok_or(SortError::NoMem)
}

/// The stable merge sort shared with the RD-24 qsort switch.
pub(crate) fn mergesort_with<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    es: usize,
    mut cmp: F,
) -> Result<(), SortError> {
    // RavenBSD: `size < PSIZE / 2` (merge.c:121) is EINVAL in the donor.
    if es < PSIZE / 2 {
        return Err(SortError::Inval);
    }
    let nmemb = base.len() / es;
    if nmemb == 0 {
        return Ok(());
    }
    let total = nmemb.checked_mul(es).ok_or(SortError::NoMem)?;
    let _capacity = checked_capacity(nmemb, es)?;
    let mut temp: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    temp.try_reserve_exact(total)
        .map_err(|_| SortError::NoMem)?;
    temp.resize(total, 0);

    let mut lo = 0usize;
    while lo < nmemb {
        let hi = core::cmp::min(lo.wrapping_add(THRESHOLD), nmemb);
        insertion_sort(base, es, lo, hi, &mut cmp);
        lo = hi;
    }
    let mut width = THRESHOLD;
    while width < nmemb {
        let mut lo = 0usize;
        while lo < nmemb {
            let mid = core::cmp::min(lo.wrapping_add(width), nmemb);
            let hi = core::cmp::min(lo.wrapping_add(width).wrapping_add(width), nmemb);
            if mid < hi {
                merge_run(base, &mut temp, es, lo, mid, hi, &mut cmp);
            }
            lo = hi;
        }
        width = width.wrapping_mul(2);
    }
    Ok(())
}

/// Stable sort of `base.len() / es` elements; `Err(Inval)` for `es < 4`, `Err(NoMem)` when the
/// temporary buffer cannot be allocated.
pub fn mergesort<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    es: usize,
    cmp: F,
) -> Result<(), SortError> {
    mergesort_with(base, es, cmp)
}

#[cfg(test)]
#[path = "mergesort_ut.rs"]
mod tests;
