// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `closefrom` and `close_range` over the libc fd table (`11` 3.9 T2).
//!
//! `closefrom` closes every open descriptor `>= lo`; a negative `lo` is treated as zero. The
//! single-close failures of the loop are ignored, as the BSD man page requires.
//! `close_range` accepts only `CLOSE_RANGE_CLOEXEC`; `CLOSE_RANGE_UNSHARE` is `EINVAL` because
//! Raven has no descriptor-table unsharing.

use crate::seams::{Errno, FdTable};

/// `CLOSE_RANGE_UNSHARE`.
pub const CLOSE_RANGE_UNSHARE: u32 = 2;
/// `CLOSE_RANGE_CLOEXEC`.
pub const CLOSE_RANGE_CLOEXEC: u32 = 4;

/// `closefrom(lo)`.
pub fn closefrom<T: FdTable>(table: &mut T, lo: i32) {
    let lo = lo.max(0);
    let _ = table.close_range_from(lo);
}

/// `close_range(lo, hi, flags)`.
pub fn close_range<T: FdTable>(table: &mut T, lo: u32, hi: u32, flags: u32) -> Result<(), Errno> {
    if flags & !CLOSE_RANGE_CLOEXEC != 0 {
        return Err(Errno::EINVAL);
    }
    if lo > hi {
        return Err(Errno::EINVAL);
    }
    let lo = i32::try_from(lo).unwrap_or(i32::MAX);
    let hi = i32::try_from(hi).unwrap_or(i32::MAX);
    if flags & CLOSE_RANGE_CLOEXEC != 0 {
        let mut fd = lo;
        loop {
            if table.set_cloexec(fd, true).is_err() {
                // A closed descriptor is not an error for the range operation.
            }
            if fd == hi {
                break;
            }
            fd = match fd.checked_add(1) {
                Some(next) => next,
                None => break,
            };
        }
        return Ok(());
    }
    let _ = table.close_range_between(lo, hi);
    Ok(())
}

#[cfg(test)]
#[path = "closefrom_ut.rs"]
mod tests;
