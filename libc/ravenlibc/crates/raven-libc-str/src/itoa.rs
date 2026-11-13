// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Integer to decimal bytes without `core::fmt` (R-5).
//!
//! `strerror_r`, `strerror` and `strsignal` need `"Unknown error: N"`; formatting an integer with
//! `write!` would pull the formatting machinery into every libc link, so the two functions here
//! are the whole implementation. They write plain decimal ASCII and never allocate.

/// Write `v` as decimal ASCII into `buf` and return the number of bytes written.
///
/// The caller provides room for at least 20 bytes (`i64::MIN` is 20 characters); a shorter buffer
/// receives the leading bytes only and the return value is the number of bytes actually written.
#[must_use]
pub fn write_i64(buf: &mut [u8], v: i64) -> usize {
    if v < 0 {
        if let Some(slot) = buf.first_mut() {
            *slot = b'-';
        }
        let rest = match buf.get_mut(1..) {
            Some(rest) => rest,
            None => return usize::from(!buf.is_empty()),
        };
        let written = write_u64(rest, v.unsigned_abs());
        return written.wrapping_add(1).min(buf.len());
    }
    write_u64(buf, v as u64)
}

/// Write `v` as decimal ASCII into `buf` and return the number of bytes written.
#[must_use]
pub fn write_u64(buf: &mut [u8], mut v: u64) -> usize {
    // 20 digits cover u64::MAX; the array is a local below the 128-byte by-value copy threshold.
    let mut digits = [0u8; 20];
    let mut count = 0usize;
    loop {
        let digit = v.wrapping_rem(10);
        v = v.wrapping_div(10);
        if let Some(slot) = digits.get_mut(count) {
            *slot = b'0'.wrapping_add(digit as u8);
        }
        count = count.wrapping_add(1);
        if v == 0 {
            break;
        }
    }

    let mut written = 0usize;
    let mut index = count;
    while index != 0 {
        index = index.wrapping_sub(1);
        let Some(&digit) = digits.get(index) else {
            break;
        };
        if let Some(slot) = buf.get_mut(written) {
            *slot = digit;
        } else {
            break;
        }
        written = written.wrapping_add(1);
    }
    written
}

#[cfg(test)]
#[path = "itoa_ut.rs"]
mod tests;
