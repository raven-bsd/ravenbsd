// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-proto-fs` packed directory entry tests.

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
}

#[test]
fn non_utf8_names_are_byte_strings_and_round_trip() {
    let mut bytes = [0u8; 32];
    let written = entry(&[0xff, 0xfe]).encode(&mut bytes).unwrap();
    let (decoded, consumed) = DirEntry::decode(&bytes[..written]).unwrap();
    assert_eq!(decoded.name, &[0xff, 0xfe]);
    assert_eq!(consumed, written);
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
