// SPDX-License-Identifier: BSD-2-Clause
// Copyright (C) 2014 The Android Open Source Project
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from bionic:libc/bionic/__gnu_basename.cpp @ 731631f300090436d7f5df80d50b6275c8c60a93
// (Android BSD-2 header read; no per-file revision tag)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
//  * Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
//  * Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in
//    the documentation and/or other materials provided with the
//    distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
// FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
// COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
// BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS
// OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
// AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
// OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! GNU `basename` semantics, exported as `__gnu_basename` (A-str-8).

/// Offset of the last path component: one past the last `/`, or 0 when there is none.
///
/// A trailing slash therefore yields the offset of the terminating NUL (the empty string), as the
/// GNU function does. The POSIX `basename` (which strips trailing slashes) belongs to C41.
#[must_use]
pub fn offset(path: &[u8]) -> usize {
    let mut result = 0usize;
    let mut index = 0usize;
    while index < path.len() {
        if path.get(index).copied() == Some(b'/') {
            result = index.wrapping_add(1);
        }
        index = index.wrapping_add(1);
    }
    result
}

#[cfg(test)]
#[path = "gnu_basename_ut.rs"]
mod tests;
