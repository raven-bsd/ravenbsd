// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/string/strnlen.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header; covered by musl/COPYRIGHT, H-1)
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! `strnlen` pointer entry point (`memchr(s, 0, n)` semantics).

/// Length of the C string at `s`, at most `n` bytes; never reads byte `n`.
///
/// # Safety
///
/// `s` is readable for `n` bytes.
pub unsafe fn strnlen(s: *const u8, n: usize) -> usize {
    // SAFETY: `s` is readable for `n` bytes, so the bounded `memchr` stays inside the object.
    let found = unsafe { super::memchr::memchr(s, 0, n) };
    if found.is_null() {
        n
    } else {
        (found as usize).wrapping_sub(s as usize)
    }
}
