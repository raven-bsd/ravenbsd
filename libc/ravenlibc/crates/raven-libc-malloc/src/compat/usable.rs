// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `malloc_usable_size` honesty helpers.

/// Whether a reported usable size is acceptable for a request of `requested` bytes.
#[must_use]
pub fn usable_for(usable: usize, requested: usize) -> bool {
    usable >= requested
}

#[cfg(test)]
#[path = "usable_ut.rs"]
mod tests;
