// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/string/strdup.c and strndup.c
// @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header; covered by musl/COPYRIGHT, H-1). The `malloc` symbol is a caller
// parameter here (RD-new: allocator parameter), so this crate never declares it.
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

//! `strdup`/`strndup` with an explicit allocator.

/// Type of the `malloc` function the caller passes (C `void *(*)(size_t)`).
pub type AllocFn = unsafe fn(usize) -> *mut u8;

/// Copy the C string at `s` into a fresh allocation; null when `alloc` returns null.
///
/// # Safety
///
/// `s` is a NUL-terminated string; `alloc` is a valid `malloc`-compatible function that returns
/// `size` writable bytes or null.
pub unsafe fn strdup_with(alloc: AllocFn, s: *const u8) -> *mut u8 {
    // SAFETY: `strlen` stays inside `s`.
    let length = unsafe { super::strlen::strlen(s) };
    let size = length.wrapping_add(1);
    // SAFETY: the caller guarantees `alloc` returns `size` writable bytes or null.
    let dest = unsafe { alloc(size) };
    if dest.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `dest` is writable for `size` bytes and `s` is readable for `size` bytes
    // (including the NUL).
    unsafe {
        let mut index = 0usize;
        while index < size {
            *dest.add(index) = *s.add(index);
            index = index.wrapping_add(1);
        }
    }
    dest
}

/// Copy at most `n` bytes of the C string at `s` into a fresh allocation; null on failure.
///
/// # Safety
///
/// `s` is readable for `n` bytes; `alloc` is a valid `malloc`-compatible function.
pub unsafe fn strndup_with(alloc: AllocFn, s: *const u8, n: usize) -> *mut u8 {
    // SAFETY: `strnlen` reads at most `n` bytes and never past a NUL.
    let length = unsafe { super::strnlen::strnlen(s, n) };
    let size = length.wrapping_add(1);
    // SAFETY: the caller guarantees `alloc` returns `size` writable bytes or null.
    let dest = unsafe { alloc(size) };
    if dest.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `dest` is writable for `size` bytes; `length` bytes of `s` are readable and the
    // last destination byte receives the NUL.
    unsafe {
        let mut index = 0usize;
        while index < length {
            *dest.add(index) = *s.add(index);
            index = index.wrapping_add(1);
        }
        *dest.add(length) = 0;
    }
    dest
}
