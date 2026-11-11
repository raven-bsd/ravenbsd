// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure helpers for `nulld`.
//!
//! `null:` reads always return end-of-file; writes are discarded.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// A `null:` read produces no bytes.
pub fn read_len() -> usize {
    0
}

/// Writes are discarded and reported as complete.
pub fn discard_write(buffer: &[u8]) -> usize {
    buffer.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_eof() {
        assert_eq!(read_len(), 0);
    }

    #[test]
    fn writes_are_discarded_but_counted() {
        assert_eq!(discard_write(b"abc"), 3);
        assert_eq!(discard_write(b""), 0);
    }
}
