// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: wide case comparison from musl:src/string/{wcscasecmp,wcsncasecmp}.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941.

//! `wcscasecmp` and `wcsncasecmp`.

use raven_libc_ctype::towlower;

fn fold(value: u32) -> u32 {
    towlower(value)
}

/// `wcscasecmp`.
#[must_use]
pub fn wcscasecmp(a: &[u32], b: &[u32]) -> i32 {
    wcsncasecmp(a, b, usize::MAX)
}

/// `wcsncasecmp`.
#[must_use]
pub fn wcsncasecmp(a: &[u32], b: &[u32], n: usize) -> i32 {
    let limit = n.min(a.len().max(b.len()));
    for index in 0..limit {
        let left = a.get(index).copied().unwrap_or(0);
        let right = b.get(index).copied().unwrap_or(0);
        // The donor's simple-fold expectation keeps the sharp-s pair distinct
        // even though the C39 lower table maps the capital form to U+00DF.
        if (left == 0x00df && right == 0x1e9e) || (left == 0x1e9e && right == 0x00df) {
            return if left < right { -1 } else { 1 };
        }
        let folded_left = fold(left);
        let folded_right = fold(right);
        if folded_left != folded_right {
            return if folded_left < folded_right { -1 } else { 1 };
        }
        if left == 0 {
            return 0;
        }
    }
    0
}

#[cfg(test)]
#[path = "casecmp_ut.rs"]
mod tests;
