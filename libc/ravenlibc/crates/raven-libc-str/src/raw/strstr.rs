// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2018 Rich Felker
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strstr.c and strcasestr.c, and
// freebsd:lib/libc/string/strnstr.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52 /
// 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tags: $OpenBSD: strstr.c,v 1.11 ... $OpenBSD: strcasestr.c,v 1.4 ...)
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

//! `strstr`, `strcasestr` and `strnstr` pointer entry points.

/// First occurrence of the C string `needle` in `haystack`, or null.
///
/// # Safety
///
/// Both pointers are NUL-terminated strings.
pub unsafe fn strstr(haystack: *const u8, needle: *const u8) -> *mut u8 {
    // SAFETY: both `strlen` calls stay inside their strings; the returned offset is inside the
    // haystack.
    unsafe {
        let haystack_len = super::strlen::strlen(haystack);
        let needle_len = super::strlen::strlen(needle);
        match crate::cstr::strstr(
            super::common::bytes(haystack, haystack_len),
            super::common::bytes(needle, needle_len),
        ) {
            Some(offset) => haystack.add(offset) as *mut u8,
            None => core::ptr::null_mut(),
        }
    }
}

/// First case-insensitive occurrence of `needle` in `haystack`, or null.
///
/// # Safety
///
/// Both pointers are NUL-terminated strings.
pub unsafe fn strcasestr(haystack: *const u8, needle: *const u8) -> *mut u8 {
    // SAFETY: both `strlen` calls stay inside their strings.
    unsafe {
        let haystack_len = super::strlen::strlen(haystack);
        let needle_len = super::strlen::strlen(needle);
        match crate::cstr::strcasestr(
            super::common::bytes(haystack, haystack_len),
            super::common::bytes(needle, needle_len),
        ) {
            Some(offset) => haystack.add(offset) as *mut u8,
            None => core::ptr::null_mut(),
        }
    }
}

/// First occurrence of `needle` in the first `len` bytes of `haystack`, stopping at a NUL.
///
/// # Safety
///
/// `haystack` is readable for `len` bytes; `needle` is NUL terminated.
pub unsafe fn strnstr(haystack: *const u8, needle: *const u8, len: usize) -> *mut u8 {
    // SAFETY: the caller promises `len` readable bytes of `haystack`; the needle is read to its
    // NUL. The returned offset is inside the haystack prefix.
    unsafe {
        let needle_len = super::strlen::strlen(needle);
        match crate::cstr::strnstr(
            super::common::bytes(haystack, len),
            super::common::bytes(needle, needle_len),
            len,
        ) {
            Some(offset) => haystack.add(offset) as *mut u8,
            None => core::ptr::null_mut(),
        }
    }
}
