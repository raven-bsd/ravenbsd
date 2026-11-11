// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure helpers for `zerod`.
//!
//! `zero:` reads return as many zero bytes as the client asked for; writes are discarded.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Fill `buffer` with zeros and report the number of bytes produced.
pub fn fill_zero(buffer: &mut [u8]) -> usize {
    buffer.fill(0);
    buffer.len()
}

/// Writes are discarded and reported as complete.
pub fn discard_write(buffer: &[u8]) -> usize {
    buffer.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_fill_the_whole_buffer() {
        let mut buffer = [0xAAu8; 16];
        assert_eq!(fill_zero(&mut buffer), 16);
        assert_eq!(buffer, [0u8; 16]);
    }

    #[test]
    fn empty_read_is_zero_bytes() {
        let mut buffer = [];
        assert_eq!(fill_zero(&mut buffer), 0);
    }

    #[test]
    fn writes_are_discarded_but_counted() {
        assert_eq!(discard_write(b"abc"), 3);
        assert_eq!(discard_write(b""), 0);
    }
}
