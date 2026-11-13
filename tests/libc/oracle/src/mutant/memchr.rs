// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Broken `memchr` variants.

/// Compare the haystack byte sign-extended instead of converted to unsigned.
pub fn signed(s: &[u8], c: i32) -> Option<usize> {
    s.iter().position(|&byte| i32::from(byte as i8) == c)
}

/// Never examine the last byte.
pub fn lastbyte(s: &[u8], c: i32) -> Option<usize> {
    let limit = s.len().saturating_sub(1);
    s[..limit].iter().position(|&byte| byte == c as u8)
}

/// Calls one `memchr` variant.
type MemchrMutant = fn(&[u8], i32) -> Option<usize>;

/// Every broken variant with its API name.
pub const ALL: [(&str, MemchrMutant); 2] = [("signed", signed), ("lastbyte", lastbyte)];

#[cfg(test)]
#[path = "memchr_ut.rs"]
mod tests;
