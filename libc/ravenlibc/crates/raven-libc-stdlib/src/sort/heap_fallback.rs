// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: donor index arithmetic, bounded by base.len()

//!
//! Textbook in-place sift-down heapsort over `[lo, lo + n)`.
//!
//! CLEANROOM: the donor `openbsd:lib/libc/stdlib/heapsort.c:149` allocates a key buffer and
//! `qsort.c:139-141` silently continues when that allocation fails, restoring quadratic
//! behaviour on a pathological input (A-stdlib-2). This fallback uses element swaps only, does
//! not allocate, does not recurse and has the same `O(n log n)` worst case (Williams/Floyd).

use super::qsort::swap_elems;

/// Compare the element at `i` with the one at `j`.
fn cmp_at<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &[u8],
    es: usize,
    i: usize,
    j: usize,
    cmp: &mut F,
) -> i32 {
    let left = base.get(i.wrapping_mul(es)..i.wrapping_mul(es).wrapping_add(es));
    let right = base.get(j.wrapping_mul(es)..j.wrapping_mul(es).wrapping_add(es));
    match (left, right) {
        (Some(left), Some(right)) => cmp(left, right),
        _ => 0,
    }
}

/// Sift the relative element `root` down within the relative range `[0, end)`.
fn sift_down<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    es: usize,
    lo: usize,
    root: usize,
    end: usize,
    cmp: &mut F,
) {
    let mut node = root;
    loop {
        let left = node.wrapping_mul(2).wrapping_add(1);
        if left >= end {
            break;
        }
        let mut child = left;
        if left.wrapping_add(1) < end && cmp_at(base, es, lo + left, lo + left + 1, cmp) < 0 {
            child = left.wrapping_add(1);
        }
        if cmp_at(base, es, lo + node, lo + child, cmp) >= 0 {
            break;
        }
        swap_elems(base, es, lo + node, lo + child);
        node = child;
    }
}

/// Sort `base` elements `[lo, lo + n)` in place; `base` must hold `(lo + n) * es` bytes.
pub(crate) fn heap_fallback<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    es: usize,
    lo: usize,
    n: usize,
    cmp: &mut F,
) {
    if n < 2 || es == 0 {
        return;
    }
    let mut index = n / 2;
    while index > 0 {
        index = index.wrapping_sub(1);
        sift_down(base, es, lo, index, n, cmp);
    }
    let mut end = n;
    while end > 1 {
        end = end.wrapping_sub(1);
        swap_elems(base, es, lo, lo.wrapping_add(end));
        sift_down(base, es, lo, 0, end, cmp);
    }
}

#[cfg(test)]
#[path = "heap_fallback_ut.rs"]
mod tests;
