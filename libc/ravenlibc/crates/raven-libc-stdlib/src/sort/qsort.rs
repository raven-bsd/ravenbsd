// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1992, 1993 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/qsort.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: qsort.c,v 1.12 2017/05/30 10:28:23 tb Exp $)
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

//! Bentley-McIlroy introsort with Musser's depth limit.
//!
//! Adaptations, all from the ticket: the comparator is a closure; elements are addressed by
//! index into a `&mut [u8]`; the donor's word-sized `swaptype` tricks become
//! element swaps; and at the depth limit the CLEANROOM in-place heap fallback runs, never
//! the allocating `heapsort(3)` (A-stdlib-2). Thresholds 7 and 40 are unchanged.

#![allow(clippy::arithmetic_side_effects)] // RavenBSD: donor index arithmetic, bounded by base.len()

use super::heap_fallback::heap_fallback;
use super::mergesort::mergesort_with;

/// The stability switch of RD-24: `false` selects the unstable OpenBSD introsort.
pub const STABLE_QSORT: bool = false;

/// The element at index `i`, or an empty slice when the caller's length is inconsistent.
fn elem(base: &[u8], es: usize, i: usize) -> &[u8] {
    let start = i.wrapping_mul(es);
    base.get(start..start.wrapping_add(es)).unwrap_or(&[])
}

/// Compare elements `i` and `j`.
fn cmp_at<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &[u8],
    es: usize,
    i: usize,
    j: usize,
    cmp: &mut F,
) -> i32 {
    cmp(elem(base, es, i), elem(base, es, j))
}

/// Swap two `es`-byte elements; a self-swap is a no-op (T-3).
pub(crate) fn swap_elems(base: &mut [u8], es: usize, i: usize, j: usize) {
    if i == j || es == 0 {
        return;
    }
    let (low, high) = if i < j { (i, j) } else { (j, i) };
    let low_start = low.wrapping_mul(es);
    let high_start = high.wrapping_mul(es);
    let low_end = low_start.wrapping_add(es);
    if high_start < low_end {
        return;
    }
    if high_start
        .checked_add(es)
        .is_none_or(|end| end > base.len())
    {
        return;
    }
    let (left, right) = base.split_at_mut(high_start);
    let Some(low_slice) = left.get_mut(low_start..low_end) else {
        return;
    };
    let Some(high_slice) = right.get_mut(..es) else {
        return;
    };
    low_slice.swap_with_slice(high_slice);
}

/// Swap `count` elements starting at `i` with those starting at `j`.
fn vecswap(base: &mut [u8], es: usize, i: usize, j: usize, count: usize) {
    let mut step = 0usize;
    while step < count {
        swap_elems(base, es, i.wrapping_add(step), j.wrapping_add(step));
        step = step.wrapping_add(1);
    }
}

/// The median of three element indices.
fn med3<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &[u8],
    es: usize,
    a: usize,
    b: usize,
    c: usize,
    cmp: &mut F,
) -> usize {
    if cmp_at(base, es, a, b, cmp) < 0 {
        if cmp_at(base, es, b, c, cmp) < 0 {
            b
        } else if cmp_at(base, es, a, c, cmp) < 0 {
            c
        } else {
            a
        }
    } else if cmp_at(base, es, b, c, cmp) > 0 {
        b
    } else if cmp_at(base, es, a, c, cmp) < 0 {
        a
    } else {
        c
    }
}

/// Approximate `2 * ceil(lg(n + 1))`, as `qsort.c:219-224`.
fn max_depth(n: usize) -> usize {
    let mut depth = 0usize;
    let mut value = n;
    while value > 0 {
        depth = depth.wrapping_add(1);
        value >>= 1;
    }
    depth.wrapping_mul(2)
}

fn introsort<F: FnMut(&[u8], &[u8]) -> i32>(
    base: &mut [u8],
    es: usize,
    mut lo: usize,
    mut n: usize,
    mut depth: usize,
    cmp: &mut F,
) {
    loop {
        if n < 7 {
            let mut pm = lo.wrapping_add(1);
            while pm < lo.wrapping_add(n) {
                let mut pl = pm;
                while pl > lo && cmp_at(base, es, pl.wrapping_sub(1), pl, cmp) > 0 {
                    swap_elems(base, es, pl, pl.wrapping_sub(1));
                    pl = pl.wrapping_sub(1);
                }
                pm = pm.wrapping_add(1);
            }
            return;
        }
        if depth == 0 {
            heap_fallback(base, es, lo, n, cmp);
            return;
        }
        depth = depth.wrapping_sub(1);
        let mut pm = lo.wrapping_add(n / 2);
        if n > 7 {
            let mut pl = lo;
            let mut pn = lo.wrapping_add(n).wrapping_sub(1);
            if n > 40 {
                let s = n / 8;
                pl = med3(
                    base,
                    es,
                    pl,
                    pl.wrapping_add(s),
                    pl.wrapping_add(2 * s),
                    cmp,
                );
                pm = med3(base, es, pm.wrapping_sub(s), pm, pm.wrapping_add(s), cmp);
                pn = med3(
                    base,
                    es,
                    pn.wrapping_sub(2 * s),
                    pn.wrapping_sub(s),
                    pn,
                    cmp,
                );
            }
            pm = med3(base, es, pl, pm, pn, cmp);
        }
        swap_elems(base, es, lo, pm);

        let mut pa = lo.wrapping_add(1);
        let mut pb = lo.wrapping_add(1);
        let mut pc = lo.wrapping_add(n).wrapping_sub(1);
        let mut pd = pc;
        loop {
            while pb <= pc {
                let result = cmp_at(base, es, pb, lo, cmp);
                if result > 0 {
                    break;
                }
                if result == 0 {
                    swap_elems(base, es, pa, pb);
                    pa = pa.wrapping_add(1);
                }
                pb = pb.wrapping_add(1);
            }
            while pb <= pc {
                let result = cmp_at(base, es, pc, lo, cmp);
                if result < 0 {
                    break;
                }
                if result == 0 {
                    swap_elems(base, es, pc, pd);
                    pd = pd.wrapping_sub(1);
                }
                pc = pc.wrapping_sub(1);
            }
            if pb > pc {
                break;
            }
            swap_elems(base, es, pb, pc);
            pb = pb.wrapping_add(1);
            pc = pc.wrapping_sub(1);
        }

        let pn = lo.wrapping_add(n);
        let r = core::cmp::min(pa.wrapping_sub(lo), pb.wrapping_sub(pa));
        vecswap(base, es, lo, pb.wrapping_sub(r), r);
        let s = core::cmp::min(pd.wrapping_sub(pc), pn.wrapping_sub(pd).wrapping_sub(1));
        vecswap(base, es, pb, pn.wrapping_sub(s), s);

        // RavenBSD: recurse on the smaller side, iterate on the larger (qsort.c:196-210).
        let r = pb.wrapping_sub(pa);
        let s = pd.wrapping_sub(pc);
        if r < s {
            if s > 1 {
                if r > 1 {
                    introsort(base, es, lo, r, depth, cmp);
                }
                lo = pn.wrapping_sub(s);
                n = s;
                continue;
            }
        } else if r > 1 {
            if s > 1 {
                introsort(base, es, pn.wrapping_sub(s), s, depth, cmp);
            }
            n = r;
            continue;
        }
        return;
    }
}

/// `qsort` with the RD-24 stability switch; the introsort is the fallback when the stable path
/// cannot allocate (`NoMem`) or rejects the element size (`Inval`).
pub fn qsort_with<F: FnMut(&[u8], &[u8]) -> i32>(
    stable: bool,
    base: &mut [u8],
    es: usize,
    mut cmp: F,
) {
    if es == 0 {
        return;
    }
    let n = base.len() / es;
    if n < 2 {
        return;
    }
    // RavenBSD: glibc-style fallback when the stable buffer cannot be had (RD-24).
    if stable && mergesort_with(base, es, &mut cmp).is_ok() {
        return;
    }
    let depth = max_depth(n);
    introsort(base, es, 0, n, depth, &mut cmp);
}

/// Sort `base` as `base.len() / es` elements of `es` bytes.
///
/// `base.len()` must be `nmemb * es`; the shell computes that with `checked_mul` (A-stdlib-1).
/// The sort does not allocate and is not stable (`STABLE_QSORT == false`, RD-24).
pub fn qsort<F: FnMut(&[u8], &[u8]) -> i32>(base: &mut [u8], es: usize, cmp: F) {
    qsort_with(STABLE_QSORT, base, es, cmp);
}

#[cfg(test)]
#[path = "qsort_ut.rs"]
mod tests;
