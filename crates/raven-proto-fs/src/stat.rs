// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The fixed 64-byte `FSTAT` record.

use crate::ProtoError;
use crate::wire::{Reader, Writer};

/// Encoded size of a [`Stat`] record in bytes.
pub const STAT_LEN: usize = 64;

/// One `FSTAT` record.
///
/// Timestamps are whole seconds for now; a later protocol version may add nanoseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stat {
    /// Inode number within the filesystem.
    pub ino: u64,
    /// File size in bytes.
    pub size: u64,
    /// Allocated blocks in 512-byte units.
    pub blocks: u64,
    /// Modification time, seconds since the Unix epoch.
    pub mtime: i64,
    /// Status-change time, seconds since the Unix epoch.
    pub ctime: i64,
    /// Access time, seconds since the Unix epoch.
    pub atime: i64,
    /// Linux `st_mode` value (file type and permission bits).
    pub mode: u32,
    /// Number of hard links.
    pub nlink: u32,
    /// Owner user id.
    pub uid: u32,
    /// Owner group id.
    pub gid: u32,
}

impl Stat {
    /// Encode the record into exactly [`STAT_LEN`] bytes.
    pub fn encode(&self, out: &mut [u8; STAT_LEN]) -> Result<(), ProtoError> {
        let mut writer = Writer::new(out);
        writer.put_u64(self.ino)?;
        writer.put_u64(self.size)?;
        writer.put_u64(self.blocks)?;
        writer.put_i64(self.mtime)?;
        writer.put_i64(self.ctime)?;
        writer.put_i64(self.atime)?;
        writer.put_u32(self.mode)?;
        writer.put_u32(self.nlink)?;
        writer.put_u32(self.uid)?;
        writer.put_u32(self.gid)?;
        Ok(())
    }

    /// Decode exactly one [`STAT_LEN`] record.
    ///
    /// Short or longer input is rejected; a trailing-byte record is a protocol error, not a
    /// partially decoded result.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() != STAT_LEN {
            return Err(if input.len() < STAT_LEN {
                ProtoError::Truncated
            } else {
                ProtoError::TrailingBytes
            });
        }
        let mut reader = Reader::new(input);
        let stat = Self {
            ino: reader.get_u64()?,
            size: reader.get_u64()?,
            blocks: reader.get_u64()?,
            mtime: reader.get_i64()?,
            ctime: reader.get_i64()?,
            atime: reader.get_i64()?,
            mode: reader.get_u32()?,
            nlink: reader.get_u32()?,
            uid: reader.get_u32()?,
            gid: reader.get_u32()?,
        };
        if !reader.is_empty() {
            return Err(ProtoError::TrailingBytes);
        }
        Ok(stat)
    }

    /// Build a record from a raw 64-byte slice without copying.
    ///
    /// # Errors
    ///
    /// Returns [`ProtoError::Truncated`] when fewer than 64 bytes are available.
    pub fn decode_prefix(input: &[u8]) -> Result<Self, ProtoError> {
        let bytes = input.get(..STAT_LEN).ok_or(ProtoError::Truncated)?;
        let mut reader = Reader::new(bytes);
        Ok(Self {
            ino: reader.get_u64()?,
            size: reader.get_u64()?,
            blocks: reader.get_u64()?,
            mtime: reader.get_i64()?,
            ctime: reader.get_i64()?,
            atime: reader.get_i64()?,
            mode: reader.get_u32()?,
            nlink: reader.get_u32()?,
            uid: reader.get_u32()?,
            gid: reader.get_u32()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Stat {
        Stat {
            ino: 0x0102_0304_0506_0708,
            size: 0x1112_1314_1516_1718,
            blocks: 0x2122_2324_2526_2728,
            mtime: -2,
            ctime: -3,
            atime: -4,
            mode: 0o100644,
            nlink: 2,
            uid: 1000,
            gid: 1001,
        }
    }

    #[test]
    fn golden_bytes_match_the_table() {
        let mut bytes = [0u8; STAT_LEN];
        sample().encode(&mut bytes).unwrap();
        assert_eq!(
            bytes,
            [
                0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // ino
                0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11, // size
                0x28, 0x27, 0x26, 0x25, 0x24, 0x23, 0x22, 0x21, // blocks
                0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // mtime
                0xfd, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // ctime
                0xfc, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // atime
                0xa4, 0x81, 0x00, 0x00, // mode 0o100644
                0x02, 0x00, 0x00, 0x00, // nlink
                0xe8, 0x03, 0x00, 0x00, // uid 1000
                0xe9, 0x03, 0x00, 0x00, // gid 1001
            ]
        );
    }

    #[test]
    fn roundtrip_preserves_every_field() {
        let mut bytes = [0u8; STAT_LEN];
        sample().encode(&mut bytes).unwrap();
        assert_eq!(Stat::decode(&bytes).unwrap(), sample());
    }

    #[test]
    fn short_input_is_truncated() {
        assert_eq!(
            Stat::decode(&[0u8; STAT_LEN - 1]),
            Err(ProtoError::Truncated)
        );
        assert_eq!(
            Stat::decode_prefix(&[0u8; STAT_LEN - 1]),
            Err(ProtoError::Truncated)
        );
    }

    #[test]
    fn trailing_input_is_rejected() {
        assert_eq!(
            Stat::decode(&[0u8; STAT_LEN + 1]),
            Err(ProtoError::TrailingBytes)
        );
    }

    #[test]
    fn prefix_decoder_ignores_later_bytes() {
        let mut bytes = [0u8; STAT_LEN + 8];
        let mut record = [0u8; STAT_LEN];
        sample().encode(&mut record).unwrap();
        bytes[..STAT_LEN].copy_from_slice(&record);
        assert_eq!(Stat::decode_prefix(&bytes).unwrap(), sample());
    }
}
