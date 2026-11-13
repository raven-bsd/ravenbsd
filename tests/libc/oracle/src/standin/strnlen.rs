// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Clean-room `strnlen` stand-in.

/// Length of the NUL-terminated string, capped at `maxlen`.
///
/// A NUL follows `s` in the C view; embedded zero bytes in `s` count as terminators.
pub fn strnlen(s: &[u8], maxlen: usize) -> usize {
    let limit = maxlen.min(s.len());
    for (index, &byte) in s.iter().take(limit).enumerate() {
        if byte == 0 {
            return index;
        }
    }
    limit
}

#[cfg(test)]
#[path = "strnlen_ut.rs"]
mod tests;
