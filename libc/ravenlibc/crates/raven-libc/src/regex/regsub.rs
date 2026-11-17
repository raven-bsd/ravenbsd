// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared match-array reader for the `<regex.h>` substitution exports.

use alloc::vec::Vec;

use raven_libc_regex::RegMatch;

use super::RegMatchC;

/// The number of `\1`..`\9` references the engine consults.
pub(crate) const MATCHES: usize = 10;

/// Read the caller's match array.
///
/// # Safety
/// `pmatch` has at least `MATCHES` readable entries.
pub(crate) unsafe fn matches(pmatch: *const RegMatchC) -> Vec<RegMatch> {
    let mut out = Vec::with_capacity(MATCHES);
    for index in 0..MATCHES {
        // SAFETY: the caller's array.
        let value = unsafe { &*pmatch.add(index) };
        out.push(RegMatch {
            rm_so: value.rm_so,
            rm_eo: value.rm_eo,
        });
    }
    out
}
