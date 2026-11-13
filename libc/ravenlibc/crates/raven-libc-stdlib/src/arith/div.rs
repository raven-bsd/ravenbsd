// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/div.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: div.c,v 1.6 2015/09/13 08:31:47 guenther Exp $)
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

//! `div`: quotient and remainder truncated toward zero.

/// C `div_t`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct DivT {
    /// Quotient.
    pub quot: i32,
    /// Remainder.
    pub rem: i32,
}

/// `div(n, d)`. Rust `/` and `%` already truncate toward zero, so the donor's old-compiler
/// adjustment is not translated. `MIN / -1` uses wrapping operations (A-stdlib-12); a zero
/// divisor is C undefined behaviour and deliberately traps.
#[must_use]
pub fn div(n: i32, d: i32) -> DivT {
    // RavenBSD: wrapping_div/rem for MIN / -1, which is UB in C (A-stdlib-12).
    DivT {
        quot: n.wrapping_div(d),
        rem: n.wrapping_rem(d),
    }
}

#[cfg(test)]
#[path = "div_ut.rs"]
mod tests;
