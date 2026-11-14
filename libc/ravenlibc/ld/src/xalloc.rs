// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/xmalloc.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! The small allocation helpers the loader uses for strings and tables.

use crate::ldalloc::LdAlloc;

/// Allocate zeroed bytes.
pub fn xcalloc(alloc: &mut LdAlloc, count: usize, size: usize) -> Option<usize> {
    let total = count.checked_mul(size)?;
    alloc.alloc(total, 16)
}

/// Allocate and copy a byte string, including its NUL.
pub fn xstrdup(alloc: &mut LdAlloc, text: &[u8]) -> Option<usize> {
    let total = text.len().checked_add(1)?;
    let at = alloc.alloc(total, 1)?;
    Some(at)
}

/// Allocate one object of `size`.
pub fn xmalloc(alloc: &mut LdAlloc, size: usize) -> Option<usize> {
    alloc.alloc(size, 16)
}

#[cfg(test)]
#[path = "xalloc_ut.rs"]
mod tests;
