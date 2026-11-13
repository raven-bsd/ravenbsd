// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/memccpy.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: memccpy.c,v 1.7 2015/08/31 02:53:57 guenther Exp $)
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

//! `memccpy` over slices.

/// Copy `src` into `dst` and stop after the first byte equal to `c`.
///
/// At most `min(dst.len(), src.len())` bytes are copied, exactly as the C `n` bounds both sides.
/// Returns `Some(index after the copied c)` when the byte was found, `None` otherwise. The
/// comparison is on the byte value (`(unsigned char)c` at the call site).
#[must_use]
pub fn ccopy(dst: &mut [u8], src: &[u8], c: u8) -> Option<usize> {
    let len = if dst.len() < src.len() {
        dst.len()
    } else {
        src.len()
    };
    let mut index = 0usize;
    while index < len {
        let Some(&byte) = src.get(index) else {
            break;
        };
        if let Some(slot) = dst.get_mut(index) {
            *slot = byte;
        }
        index = index.wrapping_add(1);
        if byte == c {
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
#[path = "memccpy_ut.rs"]
mod tests;
