// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `umaxabs` from `<inttypes.h>`.
//!
//! `wcstoimax`/`wcstoumax` are implemented in the wchar slice; keeping a second definition here
//! caused a duplicate symbol after the P1b/P1h merge.

/// `umaxabs`: the absolute value of an `intmax_t` as a `uintmax_t`.
#[unsafe(no_mangle)]
pub extern "C" fn umaxabs(value: i64) -> u64 {
    value.unsigned_abs()
}
