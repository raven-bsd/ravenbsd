// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/memmem.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: memmem.c,v 1.6 2026/06/23 13:09:11 tim Exp $)
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

//! `memmem` pointer entry point.

/// First occurrence of the `l` bytes at `n0` in the `k` bytes at `h0`, or null.
///
/// # Safety
///
/// `h0` is readable for `k` bytes and `n0` for `l` bytes.
pub unsafe fn memmem(h0: *const u8, k: usize, n0: *const u8, l: usize) -> *mut u8 {
    // SAFETY: the caller provides both exact read extents; an empty needle is handled by the
    // engine without dereferencing `n0`.
    let (haystack, needle) = unsafe { (super::common::bytes(h0, k), super::common::bytes(n0, l)) };
    match crate::mem::find_sub(haystack, needle) {
        // SAFETY: the engine returned an offset inside `haystack` (0..=k), so `h0 + offset` is
        // inside or one past the caller's object.
        Some(offset) => unsafe { h0.add(offset) as *mut u8 },
        None => core::ptr::null_mut(),
    }
}
