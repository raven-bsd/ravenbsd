// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
//! The `__ssp_overlap` range-intersection test.

/// Whether `[left, left + sz)` and `[right, right + sz)` intersect.
#[must_use]
pub fn ssp_overlap(left: usize, right: usize, sz: usize) -> bool {
    if left <= right {
        usize::MAX.wrapping_sub(sz) < left || right < left.wrapping_add(sz)
    } else {
        usize::MAX.wrapping_sub(sz) < right || left < right.wrapping_add(sz)
    }
}

#[cfg(test)]
#[path = "overlap_ut.rs"]
mod tests;
