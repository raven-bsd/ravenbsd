// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2015-2016 Nuxi, https://nuxi.nl/
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/gen/basename.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (the Nuxi file carries no per-file revision tag)
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! POSIX `basename` over the caller's path bytes.

use super::GenName;

/// The donor's in-place algorithm, expressed as the resulting prefix.
#[must_use]
pub fn basename(path: &[u8]) -> GenName {
    if path.is_empty() {
        return GenName::Static(b".");
    }
    let mut end = path.len();
    while end > 1 && path.get(end.wrapping_sub(1)) == Some(&b'/') {
        end = end.wrapping_sub(1);
    }
    let mut start = end.wrapping_sub(1);
    while start > 0 && path.get(start.wrapping_sub(1)) != Some(&b'/') {
        start = start.wrapping_sub(1);
    }
    GenName::Slice {
        start,
        len: end.wrapping_sub(start),
    }
}

#[cfg(test)]
#[path = "basename_ut.rs"]
mod tests;
