// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure ring buffer and record formatting for `logd`.
//!
//! The server keeps a fixed 64 KiB ring of text records. Cursors are absolute byte positions:
//! `read` clamps a cursor that fell behind the oldest retained byte, so a slow reader sees the
//! retained tail instead of an error. This module has no syscalls and is host-tested.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

/// Ring capacity in bytes.
pub const RING_SIZE: usize = 64 * 1024;
/// Maximum encoded record length in bytes, including the trailing newline.
pub const MAX_RECORD: usize = 1024;

/// A fixed-capacity byte ring with absolute read cursors.
pub struct Ring {
    data: Vec<u8>,
    total: u64,
    len: usize,
}

impl Ring {
    /// Create a zeroed ring.
    pub fn new() -> Self {
        Self {
            data: vec![0u8; RING_SIZE],
            total: 0,
            len: 0,
        }
    }

    /// Number of retained bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the ring is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Absolute position of the oldest retained byte.
    pub fn oldest(&self) -> u64 {
        self.total.saturating_sub(self.len as u64)
    }

    /// Total bytes ever appended (before overwriting).
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Drop all retained bytes and reset the absolute counters.
    pub fn clear(&mut self) {
        self.total = 0;
        self.len = 0;
    }

    /// Append a record, overwriting the oldest bytes when the ring is full.
    ///
    /// A record larger than the ring keeps only its last [`RING_SIZE`] bytes.
    pub fn push(&mut self, record: &[u8]) {
        for byte in record {
            let position = (self.total % RING_SIZE as u64) as usize;
            if let Some(slot) = self.data.get_mut(position) {
                *slot = *byte;
            }
            self.total = self.total.wrapping_add(1);
            if self.len < RING_SIZE {
                self.len += 1;
            }
        }
    }

    /// Read up to `out.len()` retained bytes starting at `cursor`.
    ///
    /// Returns the number of bytes copied and the next absolute cursor. A cursor older than the
    /// oldest retained byte is advanced to [`Ring::oldest`].
    pub fn read(&self, cursor: u64, out: &mut [u8]) -> (usize, u64) {
        let cursor = cursor.max(self.oldest());
        let available = self.total.saturating_sub(cursor);
        let length = usize::try_from(available)
            .unwrap_or(usize::MAX)
            .min(out.len());
        for (index, slot) in out.iter_mut().take(length).enumerate() {
            let position = ((cursor + index as u64) % RING_SIZE as u64) as usize;
            *slot = self.data.get(position).copied().unwrap_or(0);
        }
        (length, cursor.saturating_add(length as u64))
    }
}

impl Default for Ring {
    fn default() -> Self {
        Self::new()
    }
}

/// Format one log record: `<seconds>.<millis> <name>: <message>\\n`.
///
/// The message's trailing newline is removed and exactly one is added. A record longer than
/// [`MAX_RECORD`] is truncated to `MAX_RECORD - 4` bytes and ends with `...\\n`.
pub fn format_record(seconds: u64, millis: u32, name: &str, message: &[u8]) -> Vec<u8> {
    let millis = millis % 1000;
    let prefix = alloc::format!("{seconds}.{millis:03} {name}: ");
    let mut text = prefix.into_bytes();
    let mut body = message;
    while body.ends_with(b"\n") || body.ends_with(b"\r") {
        body = body.get(..body.len() - 1).unwrap_or(&[]);
    }
    text.extend_from_slice(body);
    if text.len() >= MAX_RECORD {
        text.truncate(MAX_RECORD - 4);
        text.extend_from_slice(b"...\n");
    } else {
        text.push(b'\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(data: &[u8]) -> &str {
        core::str::from_utf8(data).unwrap()
    }

    #[test]
    fn new_ring_is_empty() {
        let ring = Ring::new();
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.oldest(), 0);
        assert_eq!(ring.total(), 0);
    }

    #[test]
    fn push_and_read_returns_bytes() {
        let mut ring = Ring::new();
        ring.push(b"hello");
        let mut out = [0u8; 8];
        assert_eq!(ring.read(0, &mut out), (5, 5));
        assert_eq!(&out[..5], b"hello");
    }

    #[test]
    fn cursor_advances_across_reads() {
        let mut ring = Ring::new();
        ring.push(b"abcdef");
        let mut out = [0u8; 3];
        let (first, cursor) = ring.read(0, &mut out);
        assert_eq!((first, cursor), (3, 3));
        let (second, cursor) = ring.read(cursor, &mut out);
        assert_eq!((second, cursor), (3, 6));
        assert_eq!(ring.read(cursor, &mut out), (0, 6));
    }

    #[test]
    fn read_at_end_is_eof() {
        let mut ring = Ring::new();
        ring.push(b"x");
        let mut out = [0u8; 4];
        assert_eq!(ring.read(1, &mut out), (0, 1));
    }

    #[test]
    fn old_cursor_is_clamped_to_oldest() {
        let mut ring = Ring::new();
        let data = vec![b'a'; RING_SIZE + 10];
        ring.push(&data);
        assert_eq!(ring.oldest(), 10);
        let mut out = [0u8; 4];
        let (length, cursor) = ring.read(0, &mut out);
        assert_eq!(length, 4);
        assert_eq!(cursor, 14);
    }

    #[test]
    fn ring_wraps_in_byte_order() {
        let mut ring = Ring::new();
        ring.push(&vec![b'a'; RING_SIZE - 2]);
        ring.push(b"bcdef");
        let mut out = vec![0u8; 5];
        let (length, _) = ring.read(ring.total() - 5, &mut out);
        assert_eq!(length, 5);
        assert_eq!(&out[..], b"bcdef");
    }

    #[test]
    fn overwrite_keeps_only_the_tail() {
        let mut ring = Ring::new();
        ring.push(&vec![1u8; RING_SIZE]);
        ring.push(&[2u8; 4]);
        let mut out = vec![0u8; 8];
        let (length, _) = ring.read(ring.total() - 8, &mut out);
        assert_eq!(length, 8);
        assert_eq!(&out[..4], &[1u8; 4]);
        assert_eq!(&out[4..], &[2u8; 4]);
    }

    #[test]
    fn clear_resets_counters() {
        let mut ring = Ring::new();
        ring.push(b"abc");
        ring.clear();
        assert!(ring.is_empty());
        assert_eq!(ring.total(), 0);
        assert_eq!(ring.oldest(), 0);
    }

    #[test]
    fn oversized_record_keeps_last_ring_bytes() {
        let mut ring = Ring::new();
        ring.push(&vec![7u8; RING_SIZE + 5]);
        assert_eq!(ring.len(), RING_SIZE);
        assert_eq!(ring.oldest(), 5);
        assert_eq!(ring.total() as usize, RING_SIZE + 5);
    }

    #[test]
    fn record_starts_with_time_and_name() {
        let record = format_record(12, 34, "sysd", b"started\n");
        assert_eq!(text(&record), "12.034 sysd: started\n");
    }

    #[test]
    fn millis_are_three_digits() {
        assert_eq!(text(&format_record(1, 5, "a", b"x")), "1.005 a: x\n");
        assert_eq!(text(&format_record(1, 999, "a", b"x")), "1.999 a: x\n");
    }

    #[test]
    fn message_newlines_are_normalized() {
        assert_eq!(text(&format_record(0, 0, "a", b"x\n\n")), "0.000 a: x\n");
        assert_eq!(text(&format_record(0, 0, "a", b"x")), "0.000 a: x\n");
    }

    #[test]
    fn empty_message_still_has_a_record() {
        assert_eq!(text(&format_record(0, 0, "a", b"")), "0.000 a: \n");
    }

    #[test]
    fn exactly_max_record_is_not_truncated() {
        let prefix = "0.000 a: ";
        let body = vec![b'x'; MAX_RECORD - prefix.len() - 1];
        let record = format_record(0, 0, "a", &body);
        assert_eq!(record.len(), MAX_RECORD);
        assert!(record.ends_with(b"\n"));
        assert!(!record.ends_with(b"...\n"));
    }

    #[test]
    fn long_record_is_truncated_with_ellipsis() {
        let record = format_record(0, 0, "a", &vec![b'x'; MAX_RECORD * 2]);
        assert_eq!(record.len(), MAX_RECORD);
        assert!(record.ends_with(b"...\n"));
    }

    #[test]
    fn truncation_keeps_the_prefix() {
        let record = format_record(123, 456, "logd", &vec![b'x'; MAX_RECORD * 2]);
        assert!(record.starts_with(b"123.456 logd: "));
    }
}
