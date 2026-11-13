// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/strtol.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strtol.c,v 1.12 2017/07/06 16:23:11 millert Exp $)
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

//! `strtol`, and the `strtoll`/`strtoimax` aliases (one `i64` instantiation).

use super::scan::scan_integer;
use crate::IntStatus;

/// Convert the leading integer in `s` (no NUL) with `base`.
///
/// Returns `(value, consumed, status)`. `Range` means the shell sets `ERANGE` and the value is
/// saturated to `LONG_MIN`/`LONG_MAX`; `InvalidBase` means `EINVAL` with `consumed == 0`;
/// `NoDigits` means no conversion with `consumed == 0` and no `errno` change.
#[must_use]
pub fn strtol(s: &[u8], base: i32) -> (i64, usize, IntStatus) {
    let scan = scan_integer(s, base, true);
    let value = if scan.negative {
        // RavenBSD: the C negates the magnitude after the loop; wrapping covers LONG_MIN.
        (scan.value as i64).wrapping_neg()
    } else {
        scan.value as i64
    };
    (value, scan.consumed, scan.status)
}

#[cfg(test)]
#[path = "strtol_ut.rs"]
mod tests;
