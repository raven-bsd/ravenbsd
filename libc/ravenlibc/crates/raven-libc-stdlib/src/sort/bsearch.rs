// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/bsearch.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: bsearch.c,v 1.7 2015/09/13 08:31:47 guenther Exp $)
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

#![allow(clippy::arithmetic_side_effects)] // RavenBSD: donor lim-halving index arithmetic

//! `bsearch`: the donor's `lim`-halving loop, returning the index of an equal element.

/// Search `base` for the element `cmp_key_elem` reports as equal.
///
/// `base.len()` must be `nmemb * es`; the comparator is called as `cmp(key, element)` and the
/// loop halves `lim` instead of computing `(lo + hi) / 2`, so it cannot overflow.
#[must_use]
pub fn bsearch<F: FnMut(&[u8]) -> i32>(
    base: &[u8],
    es: usize,
    mut cmp_key_elem: F,
) -> Option<usize> {
    if es == 0 {
        return None;
    }
    let mut lo = 0usize;
    let mut lim = base.len() / es;
    while lim != 0 {
        let mid = lo.wrapping_add(lim >> 1);
        let start = mid.wrapping_mul(es);
        let element = base.get(start..start.wrapping_add(es))?;
        let result = cmp_key_elem(element);
        if result == 0 {
            return Some(mid);
        }
        if result > 0 {
            lo = mid.wrapping_add(1);
            lim = lim.wrapping_sub(1);
        }
        lim >>= 1;
    }
    None
}

#[cfg(test)]
#[path = "bsearch_ut.rs"]
mod tests;
