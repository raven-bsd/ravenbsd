// SPDX-License-Identifier: ISC
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/recallocarray.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// Copyright (c) 2008, 2017 Otto Moerbeek <otto@drijf.net>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: derived from openbsd:lib/libc/stdlib/recallocarray.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: recallocarray.c,v 1.2 2021/03/18 11:16:58 claudio Exp $)
// The full ISC permission text is in hardened/mod.rs.

//! `reallocarray` and `recallocarray` overflow planning.

use crate::compat::checks::array_size;

/// The new byte count for `reallocarray`, or `None` (the caller sets `ENOMEM`).
#[must_use]
pub fn realloc_size(n: usize, m: usize) -> Option<usize> {
    array_size(n, m)
}

/// The old and new byte counts for `recallocarray`.
///
/// The donor sets `ENOMEM` for a bad new size and `EINVAL` for a bad old size; the caller keeps
/// those distinct.
pub fn recalloc_sizes(
    old_n: usize,
    new_n: usize,
    size: usize,
) -> Result<(Option<usize>, usize), i32> {
    let new_bytes = match realloc_size(new_n, size) {
        Some(value) => value,
        None => return Err(raven_libc_abi::errno::ENOMEM),
    };
    if old_n == 0 {
        return Ok((None, new_bytes));
    }
    match realloc_size(old_n, size) {
        Some(value) => Ok((Some(value), new_bytes)),
        None => Err(raven_libc_abi::errno::EINVAL),
    }
}

#[cfg(test)]
#[path = "recallocarray_ut.rs"]
mod tests;
