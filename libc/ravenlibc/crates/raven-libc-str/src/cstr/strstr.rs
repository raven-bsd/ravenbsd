// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2018 Rich Felker
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strstr.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strstr.c,v 1.11 2026/06/23 13:10:40 tim Exp $)
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

//! `strstr` over slices: the Two-Way core of `memmem`.

/// Index of the first occurrence of the C string `needle` in `haystack`.
///
/// Both slices are string bytes without their NUL. An empty needle matches at offset 0.
#[must_use]
pub fn strstr(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    crate::mem::find_sub(haystack, needle)
}

#[cfg(test)]
#[path = "strstr_ut.rs"]
mod tests;
