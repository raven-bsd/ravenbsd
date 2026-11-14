// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `getauxval` (`X54`, `11` 3.9 T1, `R-02-3`).
//!
//! The reader is C54's auxv table (the shell supplies the lookup closure over the process's
//! retained `AT_*` values). The contract is glibc's: a key that is present with value zero
//! returns 0 and leaves `errno` alone; an absent key returns 0 with `errno = ENOENT`.

use crate::seams::Errno;

/// `AT_NULL`.
pub const AT_NULL: u64 = 0;

/// `getauxval(key, lookup)` -> `(value, errno)`, where `errno` is 0 or `ENOENT`.
#[must_use]
pub fn getauxval(key: u64, lookup: &dyn Fn(u64) -> Option<u64>) -> (u64, i32) {
    match lookup(key) {
        Some(value) => (value, 0),
        None => (0, i32::from(Errno::ENOENT.0)),
    }
}

#[cfg(test)]
#[path = "auxval_ut.rs"]
mod tests;
