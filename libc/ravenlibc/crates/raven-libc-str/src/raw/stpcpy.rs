// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/string/stpcpy.c and stpncpy.c
// @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file headers; covered by musl/COPYRIGHT, H-1). The OpenBSD `strncpy` engine is in
// `crate::cstr::strncpy`; this file is the pointer shell for all three.
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

//! `stpcpy`, `strncpy` and `stpncpy` pointer entry points.

use super::common::has_zero;

/// Copy the C string at `s` to `d`, returning a pointer to the copied NUL.
///
/// # Safety
///
/// `d` is writable for the length of `s` plus one byte; `s` is NUL terminated; no overlap.
pub unsafe fn stpcpy(d: *mut u8, s: *const u8) -> *mut u8 {
    // SAFETY: the caller's `strcpy` contract; the word loop reads only aligned source words
    // (page-safe, A-str-1) and writes a full word only when the source word has no NUL, so every
    // written byte belongs to the string.
    unsafe { stpcpy_inner(d, s) }
}

#[cfg(any(feature = "size", raven_strict_reads))]
unsafe fn stpcpy_inner(d: *mut u8, s: *const u8) -> *mut u8 {
    // SAFETY: the caller's `strcpy` contract; one byte at a time until and including the NUL.
    unsafe {
        let mut source = s;
        let mut dest = d;
        loop {
            let byte = *source;
            *dest = byte;
            if byte == 0 {
                return dest;
            }
            source = source.add(1);
            dest = dest.add(1);
        }
    }
}

#[cfg(not(any(feature = "size", raven_strict_reads)))]
unsafe fn stpcpy_inner(d: *mut u8, s: *const u8) -> *mut u8 {
    // SAFETY: the caller's `strcpy` contract; the word loop reads aligned source words
    // (page-safe, A-str-1) and writes a full word only when the source word has no NUL.
    unsafe {
        const WORD: usize = core::mem::size_of::<usize>();
        const MASK: usize = WORD - 1;

        let mut source = s;
        let mut dest = d;
        if (source as usize) & MASK == (dest as usize) & MASK {
            while (source as usize) & MASK != 0 {
                let byte = *source;
                *dest = byte;
                if byte == 0 {
                    return dest;
                }
                source = source.add(1);
                dest = dest.add(1);
            }
            loop {
                // SAFETY: `source` is aligned; an aligned word load cannot cross a page boundary
                // (A-str-1). The word is written only when it contains no NUL, and `dest` has
                // the same alignment.
                let word = core::ptr::read_volatile(source as *const usize);
                if has_zero(word) {
                    break;
                }
                core::ptr::write_volatile(dest as *mut usize, word);
                source = source.add(WORD);
                dest = dest.add(WORD);
            }
        }
        loop {
            let byte = *source;
            *dest = byte;
            if byte == 0 {
                return dest;
            }
            source = source.add(1);
            dest = dest.add(1);
        }
    }
}

/// Copy at most `n` bytes of the C string at `s` into `d`, NUL-padding to `n`.
///
/// # Safety
///
/// `d` is writable for `n` bytes; `s` is readable for up to `n` bytes (it stops at the NUL).
pub unsafe fn strncpy(d: *mut u8, s: *const u8, n: usize) -> *mut u8 {
    // SAFETY: `strnlen` reads at most `n` bytes without passing the NUL; `bytes_mut` is the
    // caller's `n`-byte destination. The ranges do not overlap.
    unsafe {
        let length = super::strnlen::strnlen(s, n);
        let source = super::common::bytes(s, length);
        let dest = super::common::bytes_mut(d, n);
        crate::cstr::strncpy(dest, source);
    }
    d
}

/// Copy at most `n` bytes of the C string at `s` into `d`, NUL-padding to `n`, and return the
/// position of the first NUL written (or `d + n` when the source did not fit).
///
/// # Safety
///
/// `d` is writable for `n` bytes; `s` is readable for up to `n` bytes.
pub unsafe fn stpncpy(d: *mut u8, s: *const u8, n: usize) -> *mut u8 {
    // SAFETY: same contract as `strncpy`; `length <= n`, so the returned pointer is inside or
    // one past the destination object.
    unsafe {
        let length = super::strnlen::strnlen(s, n);
        let source = super::common::bytes(s, length);
        let dest = super::common::bytes_mut(d, n);
        crate::cstr::strncpy(dest, source);
        d.add(length)
    }
}
