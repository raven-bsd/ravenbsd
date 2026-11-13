// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2015 The NetBSD Foundation, Inc.
// All rights reserved.
//
// This code is derived from software contributed to The NetBSD Foundation
// by Christos Zoulas.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/regex/regsub.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// Altered: transpiled to Rust; hardening limits MAX_STRIP, BACKREF_BUDGET, MAX_PAREN_DEPTH, BACKREF_DEPTH_MAX; generic state-set engine; wide-character access through RegexLocale.

//! `regnsub` and `regasub` (`regsub.c`).
//!
//! The substitution language is the donor's: `&` and `\0`..`\9` insert a match, a backslash
//! before a backslash or `&` quotes it, and anything else is copied literally. The donor
//! writes the terminating NUL at `s_max - 1` of its growing buffer; the Rust vector result
//! carries the text length instead, and the fixed-buffer entry point reproduces the donor's
//! writes and return value, including its truncation count, without ever writing out of
//! bounds (the donor's `size_t` underflow path is a buffer overflow; it is not reproduced).

use alloc::vec::Vec;

use crate::RegError;
use crate::RegMatch;
use crate::consts::REG_ESPACE;

/// Growth granularity of the allocating variant (`REINCR`).
const REINCR: usize = 64;

/// The donor's `struct str`: a fixed caller buffer or a growing one.
struct Str {
    /// The bytes.
    buf: Vec<u8>,
    /// Current allocation (`s_max`).
    max: usize,
    /// Current text length (`s_len`).
    len: usize,
    /// The buffer belongs to the caller.
    fixed: bool,
}

impl Str {
    /// `initstr`: adopt `buf` (fixed) or allocate `max` bytes (growing).
    fn new(buf: Vec<u8>, max: usize, fixed: bool) -> Self {
        Self {
            buf,
            max,
            len: 0,
            fixed,
        }
    }

    /// `addspace`: is there room for `len` more bytes? Grow when allowed.
    fn addspace(&mut self, len: usize) -> bool {
        if self.max.saturating_sub(self.len) > len {
            return true;
        }
        if self.fixed {
            return false;
        }
        let Some(newmax) = self
            .max
            .checked_add(len)
            .and_then(|v| v.checked_add(REINCR))
        else {
            return false;
        };
        if self
            .buf
            .try_reserve(newmax.saturating_sub(self.buf.len()))
            .is_err()
        {
            return false;
        }
        self.buf.resize(newmax, 0);
        self.max = newmax;
        true
    }

    /// `addchar`.
    fn addchar(&mut self, c: u8) {
        if self.addspace(1) {
            if let Some(slot) = self.buf.get_mut(self.len) {
                *slot = c;
            }
            self.len = self.len.saturating_add(1);
        } else {
            self.len = self.len.saturating_add(1);
        }
        if c == 0 {
            self.len = self.len.saturating_sub(1);
            if let Some(slot) = self.buf.get_mut(self.max.saturating_sub(1)) {
                *slot = 0;
            }
        }
    }

    /// `addnstr`.
    fn addnstr(&mut self, source: &[u8]) {
        if self.addspace(source.len()) {
            for (index, &byte) in source.iter().enumerate() {
                let at = self.len.saturating_add(index);
                if let Some(slot) = self.buf.get_mut(at) {
                    *slot = byte;
                }
            }
        }
        self.len = self.len.saturating_add(source.len());
    }
}

/// Copy `source[rm_so..rm_eo]` defensively; a bad span yields nothing.
fn matched<'a>(source: &'a [u8], rm: &RegMatch) -> &'a [u8] {
    if rm.rm_so < 0 || rm.rm_eo < 0 || rm.rm_eo < rm.rm_so {
        return &[];
    }
    let start = rm.rm_so as usize;
    let end = rm.rm_eo as usize;
    source.get(start..end).unwrap_or(&[])
}

/// `regsub1`: the shared worker.
fn regsub1(sub: &[u8], rm: &[RegMatch], source: &[u8], mut state: Str) -> Result<Str, ()> {
    let mut at = 0usize;
    while let Some(&c) = sub.get(at) {
        if c == 0 {
            break;
        }
        at = at.saturating_add(1);
        let mut index: Option<usize> = None;
        match c {
            b'&' => index = Some(0),
            b'\\' => {
                if let Some(&next) = sub.get(at)
                    && next.is_ascii_digit()
                {
                    index = Some(usize::from(next.wrapping_sub(b'0')));
                    at = at.saturating_add(1);
                }
            }
            _ => {}
        }
        if let Some(index) = index {
            if let Some(entry) = rm.get(index) {
                state.addnstr(matched(source, entry));
            }
        } else {
            let mut literal = c;
            if c == b'\\'
                && let Some(&next) = sub.get(at)
                && (next == b'\\' || next == b'&')
            {
                literal = next;
                at = at.saturating_add(1);
            }
            state.addchar(literal);
        }
    }
    state.addchar(0);
    if !state.fixed && state.len >= state.max {
        return Err(());
    }
    Ok(state)
}

/// `regnsub`: substitute into the caller's fixed buffer.
///
/// Returns the donor's `s_len`: the length the substituted text would have, even when the
/// fixed buffer was too small (the surplus is counted, not written). The bytes of `buf` are
/// the donor's bytes; unwritten tail bytes stay as the caller left them.
pub fn regnsub(buf: &mut [u8], sub: &[u8], pmatch: &[RegMatch], source: &[u8]) -> isize {
    let max = buf.len();
    let state = Str::new(buf.to_vec(), max, true);
    let Ok(state) = regsub1(sub, pmatch, source, state) else {
        return -1;
    };
    for (slot, &byte) in buf.iter_mut().zip(state.buf.iter()).take(max) {
        *slot = byte;
    }
    state.len as isize
}

/// `regasub`: substitute into a freshly allocated buffer.
///
/// The result is the substituted text (the donor returns its length; the NUL it writes at
/// the end of its allocation is the shell's job to append). `REG_ESPACE` when the growing
/// allocation fails.
pub fn regasub(sub: &[u8], pmatch: &[RegMatch], source: &[u8]) -> Result<Vec<u8>, RegError> {
    let state = Str::new(alloc::vec![0u8; REINCR], REINCR, false);
    match regsub1(sub, pmatch, source, state) {
        Ok(state) => {
            let mut out = Vec::new();
            if out.try_reserve(state.len).is_err() {
                return Err(RegError(REG_ESPACE));
            }
            for index in 0..state.len {
                out.push(state.buf.get(index).copied().unwrap_or(0));
            }
            Ok(out)
        }
        Err(()) => Err(RegError(REG_ESPACE)),
    }
}

#[cfg(test)]
#[path = "regsub_ut.rs"]
mod tests;
