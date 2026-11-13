// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/ldiv.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: ldiv.c,v 1.4 2015/09/13 08:31:47 guenther Exp $)
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

#![allow(clippy::arithmetic_side_effects)] // RavenBSD: divisor 0 is C UB and must trap (A-stdlib-12)

//! `ldiv` over the LP64 `long == i64` alias family.

/// C `ldiv_t`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct LDivT {
    /// Quotient.
    pub quot: i64,
    /// Remainder.
    pub rem: i64,
}

/// `ldiv(n, d)`; `lldiv` and `imaxdiv` are aliases of this body.
#[must_use]
pub fn ldiv(n: i64, d: i64) -> LDivT {
    // RavenBSD: wrapping_div/rem for MIN / -1, which is UB in C (A-stdlib-12).
    LDivT {
        quot: n.wrapping_div(d),
        rem: n.wrapping_rem(d),
    }
}

#[cfg(test)]
#[path = "ldiv_ut.rs"]
mod tests;
