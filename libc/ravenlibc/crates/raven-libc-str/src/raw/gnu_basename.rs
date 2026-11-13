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

//! `__gnu_basename` pointer entry point.

/// Pointer to the last path component (the byte after the last `/`), or `path` itself.
///
/// # Safety
///
/// `path` is a NUL-terminated string.
pub unsafe fn gnu_basename(path: *const u8) -> *mut u8 {
    // SAFETY: `strlen` stays inside the string; the offset is at most the length, so the result
    // is inside the object (including the NUL).
    unsafe {
        let length = super::strlen::strlen(path);
        let offset = crate::gnu_basename::offset(super::common::bytes(path, length));
        path.add(offset) as *mut u8
    }
}
