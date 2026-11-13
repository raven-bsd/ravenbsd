// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1989, 1993 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/lsearch.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: lsearch.c,v 1.6 2014/12/08 03:48:58 tedu Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

#![allow(clippy::arithmetic_side_effects, clippy::question_mark)] // RavenBSD: donor n*width offset

//! `lsearch`/`lfind` over a byte array.
//!
//! The donor appends with `memmove(base + *nelp * width, key, width)`; the offset is computed
//! with `checked_mul` here and an out-of-space or zero-width array is refused (A-stdlib-1).

/// Why `lsearch` could not append.

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum LSearchError {
    /// `width == 0`, or `n * width` overflows.
    Inval,
    /// The caller's array has no room for one more element.
    NoSpace,
}

/// `lfind`: the index of the first equal element.
pub fn lfind<F: FnMut(&[u8], &[u8]) -> i32>(
    key: &[u8],
    base: &[u8],
    nel: usize,
    width: usize,
    cmp: &mut F,
) -> Option<usize> {
    if width == 0 {
        return None;
    }
    let mut index = 0usize;
    while index < nel {
        let start = index.wrapping_mul(width);
        let Some(element) = base.get(start..start.wrapping_add(width)) else {
            return None;
        };
        if cmp(key, element) == 0 {
            return Some(index);
        }
        index = index.wrapping_add(1);
    }
    None
}

/// `lsearch`: find or append `key` and bump `nel`.
///
/// # Errors
///
/// [`LSearchError::Inval`] for a zero width or an overflowing offset,
/// [`LSearchError::NoSpace`] when `base` cannot hold another element.
pub fn lsearch<F: FnMut(&[u8], &[u8]) -> i32>(
    key: &[u8],
    base: &mut [u8],
    nel: &mut usize,
    width: usize,
    cmp: &mut F,
) -> Result<usize, LSearchError> {
    if width == 0 {
        return Err(LSearchError::Inval);
    }
    if let Some(index) = lfind(key, base, *nel, width, cmp) {
        return Ok(index);
    }
    let offset = nel.checked_mul(width).ok_or(LSearchError::Inval)?;
    if width > key.len() {
        return Err(LSearchError::Inval);
    }
    let Some(target) = base.get_mut(offset..offset.wrapping_add(width)) else {
        return Err(LSearchError::NoSpace);
    };
    let Some(source) = key.get(..width) else {
        return Err(LSearchError::Inval);
    };
    target.copy_from_slice(source);
    *nel = nel.wrapping_add(1);
    Ok(offset / width)
}

#[cfg(test)]
#[path = "lsearch_ut.rs"]
mod tests;
