// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Broken `strnlen` variants.

/// Ignore `maxlen` and scan the whole string.
pub fn nomax(s: &[u8], _maxlen: usize) -> usize {
    s.iter().position(|&byte| byte == 0).unwrap_or(s.len())
}

/// Allow one byte more than `maxlen`.
pub fn off1(s: &[u8], maxlen: usize) -> usize {
    let limit = maxlen.saturating_add(1).min(s.len());
    s.iter()
        .take(limit)
        .position(|&byte| byte == 0)
        .unwrap_or(limit)
}

/// Calls one `strnlen` variant.
type StrnlenMutant = fn(&[u8], usize) -> usize;

/// Every broken variant with its API name.
pub const ALL: [(&str, StrnlenMutant); 2] = [("nomax", nomax), ("off1", off1)];

#[cfg(test)]
#[path = "strnlen_ut.rs"]
mod tests;
