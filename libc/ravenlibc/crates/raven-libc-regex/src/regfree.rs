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
//	@(#)regfree.c	8.3 (Berkeley) 3/20/94
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/regex/regfree.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// Altered: transpiled to Rust; hardening limits MAX_STRIP, BACKREF_BUDGET, MAX_PAREN_DEPTH, BACKREF_DEPTH_MAX; generic state-set engine; wide-character access through RegexLocale.

//! Freeing a compiled pattern (`regfree.c`).
//!
//! The donor's `regfree` checks `re_magic == MAGIC1` and `g->magic == MAGIC2` before doing
//! anything, so a zeroed or already-freed `regex_t` is a safe no-op. In Rust the owned
//! [`Regex`] frees itself in [`Drop`]; the magic words are still cleared so
//! [`Regex::magic_ok`] reports an invalidated object and the shell keeps the same guard.

use crate::Regex;
use crate::consts::MAGIC1;
use crate::regex2::ReGuts;

impl Drop for Regex {
    fn drop(&mut self) {
        if self.magic1 != MAGIC1 {
            return;
        }
        let guts: &mut ReGuts = &mut self.guts;
        if !guts.magic_ok() {
            return;
        }
        self.magic1 = 0;
        guts.magic = 0;
        // The Vecs of the guts free themselves when `self` is dropped.
    }
}

#[cfg(test)]
#[path = "regfree_ut.rs"]
mod tests;
