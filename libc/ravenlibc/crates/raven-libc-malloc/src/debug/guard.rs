// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Guard-page threshold helper.
//!
//! The first port records the threshold and the report names it; a real unmapped page after the
//! buffer needs a second platform mapping, which is deferred until the debug profile owns one.
//! The redzone still catches an overrun at the same boundary.

/// Whether a request of `size` bytes wants a guard.
#[must_use]
pub fn wants_guard(size: usize, threshold: usize) -> bool {
    threshold != 0 && size >= threshold
}
