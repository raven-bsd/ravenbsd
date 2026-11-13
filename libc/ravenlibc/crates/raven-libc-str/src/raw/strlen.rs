// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/string/strlen.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `strlen` pointer entry point.
//!
//! RavenBSD: the word loop reads aligned words with `read_volatile` (A-str-1); under the `size`
//! feature or `cfg(raven_strict_reads)` (Miri/ASan/MTE builds) only the byte loop is compiled.

use super::common::has_zero;

/// Length of the C string at `s`.
///
/// # Safety
///
/// `s` points at a NUL-terminated string.
pub unsafe fn strlen(s: *const u8) -> usize {
    // SAFETY: the caller guarantees `s` is a NUL-terminated string; both loops stop at the first
    // zero byte and never write.
    unsafe { strlen_inner(s) }
}

#[cfg(any(feature = "size", raven_strict_reads))]
unsafe fn strlen_inner(s: *const u8) -> usize {
    // SAFETY: the caller guarantees a NUL-terminated string; the loop stops at the first NUL.
    unsafe {
        let mut p = s;
        while *p != 0 {
            p = p.add(1);
        }
        (p as usize).wrapping_sub(s as usize)
    }
}

#[cfg(not(any(feature = "size", raven_strict_reads)))]
unsafe fn strlen_inner(s: *const u8) -> usize {
    // SAFETY: the caller guarantees a NUL-terminated string; the aligned word loads are
    // page-safe (A-str-1) and `read_volatile` keeps LLVM from speculating them.
    unsafe {
        const WORD: usize = core::mem::size_of::<usize>();
        const MASK: usize = WORD - 1;

        let mut p = s;
        while (p as usize) & MASK != 0 {
            if *p == 0 {
                return (p as usize).wrapping_sub(s as usize);
            }
            p = p.add(1);
        }
        loop {
            // SAFETY: `p` is word-aligned; an aligned word load cannot cross a page boundary, so
            // it cannot fault even when it extends past the NUL (A-str-1).
            let word = core::ptr::read_volatile(p as *const usize);
            if has_zero(word) {
                break;
            }
            p = p.add(WORD);
        }
        while *p != 0 {
            p = p.add(1);
        }
        (p as usize).wrapping_sub(s as usize)
    }
}
