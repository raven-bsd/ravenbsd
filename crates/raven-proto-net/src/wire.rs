// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounds-checked little-endian reader and writer.
//!
//! Every protocol message is encoded into a caller-provided byte slice; nothing is allocated.
//! Reads never index outside the slice and writes never grow it: both return [`WireError`]
//! instead of panicking.

use core::fmt;

/// Failure while reading or writing a byte slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    /// The source slice is shorter, or the destination slice is smaller, than the value.
    Truncated,
    /// The value or length exceeds a protocol limit.
    TooLong,
    /// The value violates a protocol invariant (an empty path, a reserved flag, ...).
    Invalid,
}

impl fmt::Display for WireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Truncated => "truncated buffer",
            Self::TooLong => "value exceeds the protocol limit",
            Self::Invalid => "invalid protocol value",
        })
    }
}

/// A cursor that appends little-endian fields to a fixed slice.
pub struct Writer<'a> {
    buffer: &'a mut [u8],
    position: usize,
}

impl<'a> Writer<'a> {
    /// Wrap a destination slice. The writer starts at offset zero.
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    /// Number of bytes written so far.
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Number of bytes still available.
    pub fn remaining(&self) -> usize {
        self.buffer.len().saturating_sub(self.position)
    }

    /// Reserve `count` bytes and return them.
    fn reserve(&mut self, count: usize) -> Result<&mut [u8], WireError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(WireError::Truncated)?;
        if end > self.buffer.len() {
            return Err(WireError::Truncated);
        }
        let start = self.position;
        self.position = end;
        Ok(&mut self.buffer[start..end])
    }

    /// Append one byte.
    pub fn put_u8(&mut self, value: u8) -> Result<(), WireError> {
        self.reserve(1)?.copy_from_slice(&[value]);
        Ok(())
    }

    /// Append a 16-bit little-endian integer.
    pub fn put_u16(&mut self, value: u16) -> Result<(), WireError> {
        self.reserve(2)?.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    /// Append a 32-bit little-endian integer.
    pub fn put_u32(&mut self, value: u32) -> Result<(), WireError> {
        self.reserve(4)?.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    /// Append a 64-bit little-endian integer.
    pub fn put_u64(&mut self, value: u64) -> Result<(), WireError> {
        self.reserve(8)?.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    /// Append a signed 32-bit little-endian integer.
    pub fn put_i32(&mut self, value: i32) -> Result<(), WireError> {
        self.put_u32(value as u32)
    }

    /// Append raw bytes.
    pub fn put_bytes(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        self.reserve(bytes.len())?.copy_from_slice(bytes);
        Ok(())
    }
}

/// A cursor that reads little-endian fields from a slice.
pub struct Reader<'a> {
    buffer: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    /// Wrap a source slice. The reader starts at offset zero.
    pub const fn new(buffer: &'a [u8]) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    /// Number of bytes consumed so far.
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Number of bytes left.
    pub fn remaining(&self) -> usize {
        self.buffer.len().saturating_sub(self.position)
    }

    /// Whether every byte has been consumed.
    pub fn is_empty(&self) -> bool {
        self.position >= self.buffer.len()
    }

    /// Take the next `count` bytes.
    pub fn get_bytes(&mut self, count: usize) -> Result<&'a [u8], WireError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(WireError::Truncated)?;
        if end > self.buffer.len() {
            return Err(WireError::Truncated);
        }
        let start = self.position;
        self.position = end;
        Ok(&self.buffer[start..end])
    }

    /// Read one byte.
    pub fn get_u8(&mut self) -> Result<u8, WireError> {
        let bytes = self.get_bytes(1)?;
        Ok(bytes[0])
    }

    /// Read a 16-bit little-endian integer.
    pub fn get_u16(&mut self) -> Result<u16, WireError> {
        let bytes = self.get_bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Read a 32-bit little-endian integer.
    pub fn get_u32(&mut self) -> Result<u32, WireError> {
        let bytes = self.get_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a 64-bit little-endian integer.
    pub fn get_u64(&mut self) -> Result<u64, WireError> {
        let bytes = self.get_bytes(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read a signed 32-bit little-endian integer.
    pub fn get_i32(&mut self) -> Result<i32, WireError> {
        Ok(self.get_u32()? as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_roundtrip_in_little_endian() {
        let mut buffer = [0u8; 16];
        let mut writer = Writer::new(&mut buffer);
        writer.put_u8(0x12).unwrap();
        writer.put_u16(0x3456).unwrap();
        writer.put_u32(0x789a_bcde).unwrap();
        assert_eq!(writer.position(), 7);

        let mut reader = Reader::new(&buffer[..7]);
        assert_eq!(reader.get_u8(), Ok(0x12));
        assert_eq!(reader.get_u16(), Ok(0x3456));
        assert_eq!(reader.get_u32(), Ok(0x789a_bcde));
        assert!(reader.is_empty());
    }

    #[test]
    fn golden_bytes_are_little_endian() {
        let mut buffer = [0u8; 20];
        let mut writer = Writer::new(&mut buffer);
        writer.put_u16(1).unwrap();
        writer.put_u32(0x0102_0304).unwrap();
        writer.put_u64(0x0102_0304_0506_0708).unwrap();
        writer.put_i32(-2).unwrap();
        assert_eq!(
            &buffer[..18],
            &[
                0x01, 0x00, 0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
                0xfe, 0xff, 0xff, 0xff
            ]
        );
    }

    #[test]
    fn reading_past_the_end_is_an_error() {
        let buffer = [0u8; 3];
        let mut reader = Reader::new(&buffer);
        assert_eq!(reader.get_u16(), Ok(0));
        assert_eq!(reader.get_u16(), Err(WireError::Truncated));
        assert_eq!(reader.get_u8(), Ok(0));
        assert_eq!(reader.get_u8(), Err(WireError::Truncated));
    }

    #[test]
    fn writing_past_the_end_is_an_error() {
        let mut buffer = [0u8; 3];
        let mut writer = Writer::new(&mut buffer);
        assert_eq!(writer.put_u32(0), Err(WireError::Truncated));
        assert_eq!(writer.position(), 0);
        assert_eq!(writer.put_u16(1), Ok(()));
        assert_eq!(writer.put_u16(1), Err(WireError::Truncated));
    }

    #[test]
    fn zero_length_reads_do_not_move_the_cursor() {
        let buffer = [7u8; 2];
        let mut reader = Reader::new(&buffer);
        assert_eq!(reader.get_bytes(0), Ok(&[][..]));
        assert_eq!(reader.position(), 0);
        assert_eq!(reader.remaining(), 2);
    }

    #[test]
    fn u64_roundtrips() {
        let mut buffer = [0u8; 8];
        let mut writer = Writer::new(&mut buffer);
        writer.put_u64(u64::MAX).unwrap();
        let mut reader = Reader::new(&buffer);
        assert_eq!(reader.get_u64(), Ok(u64::MAX));
    }
}
