// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Wrapping 32-bit TCP sequence arithmetic (RFC 9293 section 3.4).

/// True when `a` precedes `b` in serial-number space.
#[must_use]
pub const fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

/// True when `a` precedes or equals `b` in serial-number space.
#[must_use]
pub const fn seq_le(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) <= 0
}

/// True when `a` follows `b` in serial-number space.
#[must_use]
pub const fn seq_gt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

/// True when `a` follows or equals `b` in serial-number space.
#[must_use]
pub const fn seq_ge(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) >= 0
}

/// Distance from `start` to `end` in bytes, wrapping.
#[must_use]
#[allow(dead_code)]
pub const fn seq_distance(start: u32, end: u32) -> u32 {
    end.wrapping_sub(start)
}

#[cfg(test)]
#[path = "seq_ut.rs"]
mod tests;
