// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/strtoul.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strtoul.c,v 1.10 2017/07/06 16:23:11 millert Exp $)
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

//! `strtoul`, and the `strtoull`/`strtoumax` aliases.

use super::scan::scan_integer;
use crate::IntStatus;

/// Convert the leading integer in `s` (no NUL) with `base` as an unsigned value.
///
/// A negative input is scanned as a magnitude and wrapped (`"-1"` is `ULONG_MAX` without a
/// range error, T-5); an overflow is `ULONG_MAX` with `Range` even when the input had a `-`
/// sign, because the donor's `any < 0` suppresses the negation.
#[must_use]
pub fn strtoul(s: &[u8], base: i32) -> (u64, usize, IntStatus) {
    let scan = scan_integer(s, base, false);
    // RavenBSD: negate only a successful scan; the C checks `any > 0` before `-acc` (T-5).
    let value = if scan.negative && scan.status != IntStatus::Range {
        scan.value.wrapping_neg()
    } else {
        scan.value
    };
    (value, scan.consumed, scan.status)
}

#[cfg(test)]
#[path = "strtoul_ut.rs"]
mod tests;
