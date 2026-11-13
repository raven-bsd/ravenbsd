// SPDX-License-Identifier: MIT
// Copyright (c) 2009-2016 by the contributors listed in CREDITS.TXT
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from rust:library/compiler-builtins/compiler-builtins/src/mem/mod.rs
// @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (no per-file header; the crate licence is MIT AND Apache-2.0 WITH LLVM-exception AND
//  (MIT OR Apache-2.0) at compiler-builtins/Cargo.toml:18; MIT elected. The MIT text of
//  library/compiler-builtins/LICENSE.txt is reproduced below.)
//
// MIT License
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! The block primitives: `memcpy`, `memmove`, `memset`, `memcmp`, `bcmp`.
//!
//! RavenBSD: the compiler-builtins `intrinsics!{}`/`#[mem_builtin]` macro is replaced by plain
//! Rust-ABI functions (RD-new 1); the `memmove` selector and `memset(c as u8)` are unchanged.
//! These five bodies use only the donor loops: no `core::ptr::copy*`, no `write_bytes`, no slice
//! equality, or they would recurse into themselves (A-str-2, ticket 5.3).

#[cfg(all(target_arch = "x86_64", not(feature = "size")))]
use crate::raw::arch::x86_64::mem as impls;
#[cfg(not(all(target_arch = "x86_64", not(feature = "size"))))]
mod impls;

/// Copy `n` bytes from `src` to `dest`; the ranges must not overlap.
///
/// # Safety
///
/// `dest` is writable for `n` bytes, `src` is readable for `n` bytes and the two ranges do not
/// overlap. A zero `n` with dangling non-null pointers does nothing and returns `dest`.
pub unsafe fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    // SAFETY: the caller guarantees the two ranges are valid and non-overlapping for `n` bytes.
    unsafe {
        impls::copy_forward(dest, src, n);
    }
    dest
}

/// Copy `n` bytes from `src` to `dest`, handling any overlap.
///
/// # Safety
///
/// `dest` is writable for `n` bytes and `src` is readable for `n` bytes; the ranges may overlap.
pub unsafe fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let delta = (dest as usize).wrapping_sub(src as usize);
    if delta >= n {
        // SAFETY: either `dest` is far enough ahead of `src`, or `src` is ahead of `dest` and
        // the subtraction wrapped; in both cases a forward copy cannot clobber a byte not yet
        // read.
        unsafe {
            impls::copy_forward(dest, src, n);
        }
    } else {
        // SAFETY: the ranges overlap and `dest > src`, so only a backward copy is correct.
        unsafe {
            impls::copy_backward(dest, src, n);
        }
    }
    dest
}

/// Fill `n` bytes at `s` with the low byte of `c`.
///
/// # Safety
///
/// `s` is writable for `n` bytes.
#[inline]
pub unsafe fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    // SAFETY: the caller guarantees `s` is writable for `n` bytes.
    unsafe {
        impls::set_bytes(s, c as u8, n);
    }
    s
}

/// Compare `n` bytes; the result is the unsigned-char difference of the first differing pair.
///
/// # Safety
///
/// Both pointers are readable for `n` bytes.
pub unsafe fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    // SAFETY: the caller guarantees both ranges are readable for `n` bytes.
    unsafe { impls::compare_bytes(a, b, n) }
}

/// BSD `bcmp`: zero for equal buffers, any non-zero value otherwise.
///
/// # Safety
///
/// Both pointers are readable for `n` bytes.
pub unsafe fn bcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    // SAFETY: the caller guarantees both ranges are readable for `n` bytes.
    unsafe { impls::compare_bytes(a, b, n) }
}
