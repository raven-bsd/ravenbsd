// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1992, 1993, 1994 Henry Spencer.
// Copyright (c) 1992, 1993, 1994
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Henry Spencer.
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
//
//	@(#)utils.h	8.3 (Berkeley) 3/20/94
// $FreeBSD: head/lib/libc/regex/utils.h 341838 2018-12-12 04:23:00Z yuripv $
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/regex/utils.h @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// Altered: transpiled to Rust; hardening limits MAX_STRIP, BACKREF_BUDGET, MAX_PAREN_DEPTH, BACKREF_DEPTH_MAX; generic state-set engine; wide-character access through RegexLocale.

//! Utility definitions of the Spencer engine (`utils.h`).
//!
//! The `NLS` branch is always the one taken: wide classification comes from the locale seam.
//! `CHAR_MIN` is pinned to the signed-`char` value of the donor so [`OUT`] cannot collide with
//! a real byte; `NC_MAX` is therefore always 256 (H-14: the host's `char` signedness is never
//! consulted).

/// `DUPMAX`, re-exported under the donor's name.
pub use crate::consts::DUPMAX;
/// The "no upper bound" marker of `{m,}`, re-exported under the donor's name.
pub use crate::consts::INFINITY;

/// `CHAR_MIN` of the donor (`signed char`).
pub const CHAR_MIN: i32 = -128;
/// `CHAR_MAX` of the donor.
pub const CHAR_MAX: i32 = 127;
/// `NC_MAX`: the full byte domain, always 256 (H-14).
pub const NC_MAX: usize = 256;

/// The number of characters held in the bitmap: 256 in byte locales, 128 in UTF-8 locales.
#[must_use]
pub const fn nc(mb_cur_max: usize) -> usize {
    if mb_cur_max == 1 { NC_MAX } else { 128 }
}

/// `(unsigned char)` cast of a wide value; truncates exactly like the donor macro.
#[must_use]
pub const fn uch(c: i32) -> u8 {
    c as u8
}

/// A non-character sentinel (`CHAR_MIN - 1`); below every valid code point in signed `wint_t`.
pub const OUT: i32 = CHAR_MIN - 1;
/// A second non-character sentinel (`CHAR_MIN - 2`).
pub const IGN: i32 = CHAR_MIN - 2;

#[cfg(test)]
#[path = "utils_ut.rs"]
mod tests;
