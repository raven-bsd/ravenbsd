// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Packed directory entries returned by `READ` on a directory handle.
//!
//! One entry is a 16-byte header followed by `name_len` name bytes, padded with zeroes to the
//! next 8-byte boundary. Protocol version 1 uses sequential directory reads: the server keeps the
//! per-open-file-description cursor and the wire `offset` must be zero. A server represents that
//! cursor as a [`DirCursor`] `(generation, index)`: it records the directory generation on the
//! first read and rejects a later read through the same description with `EINVAL` when a
//! create/unlink changed the generation. [`DirCursor`] is a server-side encoding; it is not sent
//! on the wire in protocol version 1.

use crate::ProtoError;
use crate::flags::{DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK, DT_UNKNOWN};
use crate::wire::{Reader, Writer};

/// Encoded size of a [`DirEntryHeader`] in bytes.
pub const DIRENT_HEADER_LEN: usize = 16;
/// Alignment of one packed directory entry.
pub const DIRENT_ALIGN: usize = 8;
/// Maximum name length in bytes.
pub const MAX_NAME: usize = 255;

/// The fixed header of a packed directory entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirEntryHeader {
    /// Inode number.
    pub ino: u64,
    /// Directory-entry type (`DT_*`).
    pub kind: u8,
    /// Length of the following name in bytes.
    pub name_len: u8,
    /// Zero padding.
    pub pad: [u8; 6],
}

impl DirEntryHeader {
    /// Encode the header into exactly [`DIRENT_HEADER_LEN`] bytes.
    pub fn encode(&self, out: &mut [u8]) -> Result<(), ProtoError> {
        if out.len() < DIRENT_HEADER_LEN {
            return Err(ProtoError::Truncated);
        }
        if !valid_kind(self.kind) || self.name_len == 0 || self.pad != [0; 6] {
            return Err(ProtoError::Malformed);
        }
        let mut writer = Writer::new(out);
        writer.put_u64(self.ino)?;
        writer.put_u8(self.kind)?;
        writer.put_u8(self.name_len)?;
        writer.put_bytes(&self.pad)?;
        Ok(())
    }

    /// Decode exactly one header.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() != DIRENT_HEADER_LEN {
            return Err(if input.len() < DIRENT_HEADER_LEN {
                ProtoError::Truncated
            } else {
                ProtoError::TrailingBytes
            });
        }
        let mut reader = Reader::new(input);
        let header = Self {
            ino: reader.get_u64()?,
            kind: reader.get_u8()?,
            name_len: reader.get_u8()?,
            pad: {
                let bytes = reader.get_bytes(6)?;
                let mut pad = [0u8; 6];
                pad.copy_from_slice(bytes);
                pad
            },
        };
        if !valid_kind(header.kind) || header.name_len == 0 {
            return Err(ProtoError::Malformed);
        }
        if header.pad != [0; 6] {
            return Err(ProtoError::Malformed);
        }
        Ok(header)
    }
}

/// One packed directory entry with a borrowed name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirEntry<'a> {
    /// Fixed header.
    pub header: DirEntryHeader,
    /// Name bytes, valid UTF-8 without `/` or NUL.
    pub name: &'a [u8],
}

impl<'a> DirEntry<'a> {
    /// Encoded record length for this entry.
    pub const fn encoded_len(&self) -> usize {
        record_len(self.name.len())
    }

    /// Encode the entry into `out`; returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if self.header.name_len as usize != self.name.len() {
            return Err(ProtoError::Malformed);
        }
        validate_name(self.name)?;
        let length = self.encoded_len();
        if out.len() < length {
            return Err(ProtoError::Truncated);
        }
        if !valid_kind(self.header.kind) || self.header.pad != [0; 6] {
            return Err(ProtoError::Malformed);
        }
        let mut writer = Writer::new(out);
        writer.put_u64(self.header.ino)?;
        writer.put_u8(self.header.kind)?;
        writer.put_u8(self.header.name_len)?;
        writer.put_bytes(&self.header.pad)?;
        writer.put_bytes(self.name)?;
        let padding = length - DIRENT_HEADER_LEN - self.name.len();
        let zeroes = [0u8; DIRENT_ALIGN];
        writer.put_bytes(&zeroes[..padding])?;
        Ok(length)
    }

    /// Decode one entry, returning it and the number of bytes consumed.
    pub fn decode(input: &'a [u8]) -> Result<(Self, usize), ProtoError> {
        let header = DirEntryHeader::decode_prefix(input)?;
        let total = DIRENT_HEADER_LEN + header.name_len as usize;
        let length = record_len(header.name_len as usize);
        if input.len() < length {
            return Err(ProtoError::Truncated);
        }
        let name = input
            .get(DIRENT_HEADER_LEN..total)
            .ok_or(ProtoError::Truncated)?;
        validate_name(name)?;
        let padding = input.get(total..length).ok_or(ProtoError::Truncated)?;
        if padding.iter().any(|byte| *byte != 0) {
            return Err(ProtoError::Malformed);
        }
        Ok((Self { header, name }, length))
    }
}

/// Iterator over a packed sequence of directory entries.
pub struct DirEntryIter<'a> {
    rest: &'a [u8],
}

impl<'a> DirEntryIter<'a> {
    /// Wrap one `READ` result buffer.
    pub const fn new(input: &'a [u8]) -> Self {
        Self { rest: input }
    }

    /// The bytes not yet decoded.
    pub const fn rest(&self) -> &'a [u8] {
        self.rest
    }
}

impl<'a> Iterator for DirEntryIter<'a> {
    type Item = Result<DirEntry<'a>, ProtoError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        match DirEntry::decode(self.rest) {
            Ok((entry, consumed)) => {
                self.rest = self.rest.get(consumed..).unwrap_or(&[]);
                Some(Ok(entry))
            }
            Err(error) => {
                self.rest = &[];
                Some(Err(error))
            }
        }
    }
}

/// Encoded length of a directory record with `name_len` name bytes.
///
/// `name_len` must not exceed [`MAX_NAME`]; the caller checks that before encoding.
pub const fn record_len(name_len: usize) -> usize {
    (DIRENT_HEADER_LEN + name_len + (DIRENT_ALIGN - 1)) & !(DIRENT_ALIGN - 1)
}

/// Server-side directory cursor: a generation and an entry index.
///
/// The high 32 bits are the directory generation and the low 32 bits are the
/// zero-based entry index in the server's name order.  Generation 0 with index 0
/// is the start cursor; it selects the directory's current generation.  A server
/// increments the generation on every namespace change, which invalidates old
/// cursors instead of returning moved entries.  Protocol version 1 does not carry
/// this value on the wire; servers keep it in the open-file description.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirCookie {
    generation: u32,
    index: u32,
}

impl DirCookie {
    /// The start-of-directory cookie (`0`), valid for the current generation.
    pub const START: Self = Self {
        generation: 0,
        index: 0,
    };

    /// Build a cookie for `index` in `generation`.
    pub const fn new(generation: u32, index: u32) -> Self {
        Self { generation, index }
    }

    /// Whether this is the start cookie rather than an entry index.
    pub const fn is_start(self) -> bool {
        self.generation == 0 && self.index == 0
    }

    /// The generation this cookie belongs to.
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// The zero-based entry index.
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Encode the cookie for the wire `offset` field.
    pub const fn encode(self) -> u64 {
        ((self.generation as u64) << 32) | self.index as u64
    }

    /// Decode a wire `offset` value.  Every `u64` is a syntactically valid cookie;
    /// the server validates the generation.
    pub const fn decode(raw: u64) -> Self {
        Self {
            generation: (raw >> 32) as u32,
            index: raw as u32,
        }
    }

    /// The cookie for the next entry in the same generation.
    pub fn next(self) -> Result<Self, ProtoError> {
        let index = self
            .index
            .checked_add(1)
            .ok_or(ProtoError::LengthOutOfRange)?;
        Ok(Self {
            generation: self.generation,
            index,
        })
    }
}

/// Whether `kind` is one of the documented `DT_*` values.
pub const fn valid_kind(kind: u8) -> bool {
    matches!(
        kind,
        DT_UNKNOWN | DT_FIFO | DT_CHR | DT_DIR | DT_BLK | DT_REG | DT_LNK | DT_SOCK
    )
}

fn validate_name(name: &[u8]) -> Result<(), ProtoError> {
    if name.is_empty() || name.len() > MAX_NAME {
        return Err(ProtoError::InvalidName);
    }
    if name.contains(&b'/') || name.contains(&0) {
        return Err(ProtoError::InvalidName);
    }
    core::str::from_utf8(name).map_err(|_| ProtoError::InvalidUtf8)?;
    Ok(())
}

impl DirEntryHeader {
    fn decode_prefix(input: &[u8]) -> Result<Self, ProtoError> {
        let header = input
            .get(..DIRENT_HEADER_LEN)
            .ok_or(ProtoError::Truncated)?;
        let mut reader = Reader::new(header);
        let header = Self {
            ino: reader.get_u64()?,
            kind: reader.get_u8()?,
            name_len: reader.get_u8()?,
            pad: {
                let bytes = reader.get_bytes(6)?;
                let mut pad = [0u8; 6];
                pad.copy_from_slice(bytes);
                pad
            },
        };
        if !valid_kind(header.kind) || header.name_len == 0 || header.pad != [0; 6] {
            return Err(ProtoError::Malformed);
        }
        Ok(header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> DirEntryHeader {
        DirEntryHeader {
            ino: 0x0102_0304_0506_0708,
            kind: DT_REG,
            name_len: 3,
            pad: [0; 6],
        }
    }

    fn entry(name: &[u8]) -> DirEntry<'_> {
        DirEntry {
            header: DirEntryHeader {
                ino: 7,
                kind: DT_REG,
                name_len: name.len() as u8,
                pad: [0; 6],
            },
            name,
        }
    }

    #[test]
    fn header_golden_bytes() {
        let mut bytes = [0u8; DIRENT_HEADER_LEN];
        header().encode(&mut bytes).unwrap();
        assert_eq!(
            bytes,
            [
                0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0x08, 0x03, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00
            ]
        );
    }

    #[test]
    fn entry_golden_bytes_include_alignment_padding() {
        let mut bytes = [0u8; 24];
        assert_eq!(entry(b"abc").encode(&mut bytes).unwrap(), 24);
        assert_eq!(
            bytes,
            [
                0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x03, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, b'a', b'b', b'c', 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }

    #[test]
    fn record_length_aligns_to_eight() {
        assert_eq!(record_len(1), 24);
        assert_eq!(record_len(8), 24);
        assert_eq!(record_len(9), 32);
        assert_eq!(record_len(255), 272);
        assert_eq!(record_len(0), 16);
    }

    #[test]
    fn header_and_entry_roundtrip() {
        let mut bytes = [0u8; 24];
        let written = entry(b"abc").encode(&mut bytes).unwrap();
        assert_eq!(
            DirEntryHeader::decode(&bytes[..DIRENT_HEADER_LEN]),
            Ok(entry(b"abc").header)
        );
        let (decoded, consumed) = DirEntry::decode(&bytes[..written]).unwrap();
        assert_eq!(decoded, entry(b"abc"));
        assert_eq!(consumed, written);
    }

    #[test]
    fn short_entry_is_truncated() {
        let mut bytes = [0u8; 24];
        entry(b"abc").encode(&mut bytes).unwrap();
        assert_eq!(DirEntry::decode(&bytes[..23]), Err(ProtoError::Truncated));
        assert_eq!(
            DirEntryHeader::decode(&bytes[..15]),
            Err(ProtoError::Truncated)
        );
    }

    #[test]
    fn trailing_header_bytes_are_rejected() {
        let mut bytes = [0u8; 24];
        header().encode(&mut bytes[..DIRENT_HEADER_LEN]).unwrap();
        assert_eq!(
            DirEntryHeader::decode(&bytes[..17]),
            Err(ProtoError::TrailingBytes)
        );
    }

    #[test]
    fn unknown_kind_is_rejected() {
        let mut bytes = [0u8; DIRENT_HEADER_LEN];
        header().encode(&mut bytes).unwrap();
        bytes[8] = 3;
        assert_eq!(DirEntryHeader::decode(&bytes), Err(ProtoError::Malformed));
    }

    #[test]
    fn zero_name_length_is_rejected() {
        let mut bytes = [0u8; DIRENT_HEADER_LEN];
        header().encode(&mut bytes).unwrap();
        bytes[9] = 0;
        assert_eq!(DirEntryHeader::decode(&bytes), Err(ProtoError::Malformed));
    }

    #[test]
    fn nonzero_header_padding_is_rejected() {
        let mut bytes = [0u8; DIRENT_HEADER_LEN];
        header().encode(&mut bytes).unwrap();
        bytes[10] = 1;
        assert_eq!(DirEntryHeader::decode(&bytes), Err(ProtoError::Malformed));
    }

    #[test]
    fn bad_names_are_rejected() {
        let mut bytes = [0u8; 32];
        assert_eq!(
            entry(b"a/b").encode(&mut bytes),
            Err(ProtoError::InvalidName)
        );
        assert_eq!(
            entry(b"a\0b").encode(&mut bytes),
            Err(ProtoError::InvalidName)
        );
        assert_eq!(
            entry(&[0xff, 0xfe]).encode(&mut bytes),
            Err(ProtoError::InvalidUtf8)
        );
    }

    #[test]
    fn nonzero_record_padding_is_rejected() {
        let mut bytes = [0u8; 24];
        entry(b"abc").encode(&mut bytes).unwrap();
        bytes[20] = 1;
        assert_eq!(DirEntry::decode(&bytes), Err(ProtoError::Malformed));
    }

    #[test]
    fn iterator_walks_multiple_entries() {
        let mut bytes = [0u8; 48];
        let first = entry(b"a");
        let second = entry(b"bb");
        let first_len = first.encode(&mut bytes).unwrap();
        let second_len = second.encode(&mut bytes[first_len..]).unwrap();
        let entries: Vec<DirEntry<'_>> = DirEntryIter::new(&bytes[..first_len + second_len])
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, b"a");
        assert_eq!(entries[1].name, b"bb");
    }

    #[test]
    fn iterator_stops_after_an_error() {
        let mut bytes = [0u8; 24];
        entry(b"abc").encode(&mut bytes).unwrap();
        bytes[20] = 1;
        let mut iter = DirEntryIter::new(&bytes);
        assert!(iter.next().unwrap().is_err());
        assert!(iter.next().is_none());
    }

    #[test]
    fn dir_cookie_round_trips() {
        let cookie = DirCookie::new(7, 12);
        assert_eq!(cookie.generation(), 7);
        assert_eq!(cookie.index(), 12);
        assert_eq!(DirCookie::decode(cookie.encode()), cookie);
        assert_eq!(cookie.encode(), (7u64 << 32) | 12);
        assert_eq!(DirCookie::START.encode(), 0);
        assert!(DirCookie::START.is_start());
        assert!(!cookie.is_start());
    }

    #[test]
    fn dir_cookie_next_advances_the_index() {
        let cookie = DirCookie::new(3, 8);
        assert_eq!(cookie.next().unwrap(), DirCookie::new(3, 9));
    }

    #[test]
    fn dir_cookie_overflow_is_rejected() {
        assert_eq!(
            DirCookie::new(1, u32::MAX).next(),
            Err(ProtoError::LengthOutOfRange)
        );
    }
}
