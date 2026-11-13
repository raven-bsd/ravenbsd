// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/string/bcmp.c, bcopy.c, bzero.c, index.c and rindex.c
// @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file headers; covered by musl/COPYRIGHT, H-1). The OpenBSD `bcopy.c` (113 lines,
// BSD-3) is deliberately NOT ported: `bcopy(s, d, n)` is `memmove(d, s, n)` (A-str-9).
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

//! BSD compatibility one-liners: `bcopy`, `bzero`, `index`, `rindex`.

/// `bcopy(src, dst, n)` is `memmove(dst, src, n)` — note the reversed argument order (A-str-9).
///
/// # Safety
///
/// `dst` is writable for `n` bytes and `src` is readable for `n` bytes; overlap is allowed.
pub unsafe fn bcopy(src: *const u8, dst: *mut u8, n: usize) {
    // SAFETY: `memmove`'s contract with the arguments swapped back.
    unsafe {
        super::mem::memmove(dst, src, n);
    }
}

/// `bzero(p, n)` is `memset(p, 0, n)`.
///
/// # Safety
///
/// `p` is writable for `n` bytes.
pub unsafe fn bzero(p: *mut u8, n: usize) {
    // SAFETY: `memset`'s contract.
    unsafe {
        super::mem::memset(p, 0, n);
    }
}

/// `index` is `strchr`.
///
/// # Safety
///
/// `s` is a NUL-terminated string.
pub unsafe fn index(s: *const u8, c: i32) -> *mut u8 {
    // SAFETY: forwards the caller's `strchr` contract.
    unsafe { super::strchr::strchr(s, c) }
}

/// `rindex` is `strrchr`.
///
/// # Safety
///
/// `s` is a NUL-terminated string.
pub unsafe fn rindex(s: *const u8, c: i32) -> *mut u8 {
    // SAFETY: forwards the caller's `strrchr` contract.
    unsafe { super::strchr::strrchr(s, c) }
}
