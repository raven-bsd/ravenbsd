// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Clean-room `memchr` stand-in.

/// Offset of the first byte equal to `c as u8`, or `None`.
pub fn memchr(s: &[u8], c: i32) -> Option<usize> {
    s.iter().position(|&byte| byte == c as u8)
}

#[cfg(test)]
#[path = "memchr_ut.rs"]
mod tests;
