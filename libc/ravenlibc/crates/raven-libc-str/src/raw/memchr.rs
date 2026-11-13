// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/string/memchr.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `memchr` (word loop) and `memrchr` (backward byte loop) pointer entry points.

use super::common::{has_zero, ones_times};

/// First occurrence of byte `c` in the `n` bytes at `s`, or null.
///
/// # Safety
///
/// `s` is readable for `n` bytes.
pub unsafe fn memchr(s: *const u8, c: i32, n: usize) -> *mut u8 {
    // SAFETY: the caller guarantees `s` is readable for `n` bytes; every read is bounded by `n`
    // and the word loop reads only aligned words, which cannot cross a page boundary (A-str-1).
    unsafe { memchr_inner(s, c as u8, n) }
}

#[cfg(any(feature = "size", raven_strict_reads))]
unsafe fn memchr_inner(s: *const u8, c: u8, n: usize) -> *mut u8 {
    // SAFETY: the caller guarantees `s` is readable for `n` bytes.
    unsafe {
        let mut index = 0usize;
        while index < n {
            if *s.add(index) == c {
                return s.add(index) as *mut u8;
            }
            index = index.wrapping_add(1);
        }
        core::ptr::null_mut()
    }
}

#[cfg(not(any(feature = "size", raven_strict_reads)))]
unsafe fn memchr_inner(s: *const u8, c: u8, n: usize) -> *mut u8 {
    // SAFETY: the caller guarantees `s` is readable for `n` bytes; every read is bounded by `n`
    // and the word loop reads only aligned words (A-str-1).
    unsafe {
        const WORD: usize = core::mem::size_of::<usize>();
        const MASK: usize = WORD - 1;

        let mut p = s;
        let mut left = n;
        // Align by bytes.
        while (p as usize) & MASK != 0 && left != 0 && *p != c {
            p = p.add(1);
            left = left.wrapping_sub(1);
        }
        if left != 0 && *p != c {
            let key = ones_times(c);
            let mut word = p as *const usize;
            // Word loop: a NUL byte in `*word ^ key` means a `c` byte in `*word`.
            while left >= WORD {
                // SAFETY: `word` is aligned and `left >= WORD` bytes remain, so the read stays
                // inside the caller's object.
                let value = core::ptr::read_volatile(word);
                if has_zero(value ^ key) {
                    break;
                }
                word = word.add(1);
                left = left.wrapping_sub(WORD);
            }
            p = word as *const u8;
        }
        while left != 0 && *p != c {
            p = p.add(1);
            left = left.wrapping_sub(1);
        }
        if left != 0 {
            p as *mut u8
        } else {
            core::ptr::null_mut()
        }
    }
}

/// Last occurrence of byte `c` in the `n` bytes at `s`, or null.
///
/// # Safety
///
/// `s` is readable for `n` bytes.
pub unsafe fn memrchr(s: *const u8, c: i32, n: usize) -> *mut u8 {
    let c = c as u8;
    let mut left = n;
    // SAFETY: the caller guarantees `s` is readable for `n` bytes; `left` never underflows and
    // the read at `s + left - 1` is inside the object while `left != 0`.
    unsafe {
        while left != 0 {
            left = left.wrapping_sub(1);
            if *s.add(left) == c {
                return s.add(left) as *mut u8;
            }
        }
    }
    core::ptr::null_mut()
}
