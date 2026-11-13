// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/string/strchrnul.c, strchr.c and strrchr.c
// @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file headers; covered by musl/COPYRIGHT, H-1)
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

//! `strchrnul`, `strchr` and `strrchr` pointer entry points.

use super::common::{has_zero, ones_times};

/// Pointer to the first byte equal to `c`, or to the terminating NUL.
///
/// # Safety
///
/// `s` is a NUL-terminated string.
pub unsafe fn strchrnul(s: *const u8, c: i32) -> *mut u8 {
    let c = c as u8;
    // SAFETY: the caller's string contract; the word loop reads aligned words only (A-str-1)
    // and stops at the first NUL or `c` in the byte tail.
    unsafe {
        if c == 0 {
            let length = super::strlen::strlen(s);
            return s.add(length) as *mut u8;
        }
        strchrnul_inner(s, c)
    }
}

#[cfg(any(feature = "size", raven_strict_reads))]
unsafe fn strchrnul_inner(s: *const u8, c: u8) -> *mut u8 {
    // SAFETY: the caller guarantees a NUL-terminated string.
    unsafe {
        let mut p = s;
        while *p != 0 && *p != c {
            p = p.add(1);
        }
        p as *mut u8
    }
}

#[cfg(not(any(feature = "size", raven_strict_reads)))]
unsafe fn strchrnul_inner(s: *const u8, c: u8) -> *mut u8 {
    // SAFETY: the caller guarantees a NUL-terminated string; the word loads are aligned and
    // page-safe (A-str-1).
    unsafe {
        const WORD: usize = core::mem::size_of::<usize>();
        const MASK: usize = WORD - 1;

        let mut p = s;
        while (p as usize) & MASK != 0 {
            if *p == 0 || *p == c {
                return p as *mut u8;
            }
            p = p.add(1);
        }
        let key = ones_times(c);
        loop {
            // SAFETY: `p` is aligned; an aligned word load cannot cross a page boundary.
            let word = core::ptr::read_volatile(p as *const usize);
            if has_zero(word) || has_zero(word ^ key) {
                break;
            }
            p = p.add(WORD);
        }
        while *p != 0 && *p != c {
            p = p.add(1);
        }
        p as *mut u8
    }
}

/// Pointer to the first byte equal to `c`, or null.
///
/// # Safety
///
/// `s` is a NUL-terminated string.
pub unsafe fn strchr(s: *const u8, c: i32) -> *mut u8 {
    // SAFETY: `strchrnul` returns a pointer into the same object.
    let found = unsafe { strchrnul(s, c) };
    // SAFETY: `found` points at a byte of the string (at worst the NUL).
    if unsafe { *found } == c as u8 {
        found
    } else {
        core::ptr::null_mut()
    }
}

/// Pointer to the last byte equal to `c`, or null; `c == 0` finds the terminator.
///
/// # Safety
///
/// `s` is a NUL-terminated string.
pub unsafe fn strrchr(s: *const u8, c: i32) -> *mut u8 {
    // SAFETY: `strlen` stays inside the string and `strlen + 1` includes the NUL, so `memrchr`
    // reads exactly the string and its terminator.
    unsafe {
        let length = super::strlen::strlen(s);
        super::memchr::memrchr(s, c, length.wrapping_add(1))
    }
}
