// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `FdMap` inheritance block tests.

use alloc::vec;

use raven_abi::errno::Errno;

use crate::fd::inherit::{
    ENTRY_LEN, FDMAP_MAGIC, FDMAP_VERSION, FdMap, FdMapEntry, HEADER_LEN, NO_PATH, entry_handle,
    transferable,
};
use crate::fd::ofd::OfdKind;

fn sample() -> FdMap {
    let mut first = FdMapEntry::new(0, 0x10, OfdKind::File, 0, 0);
    first.path = Some(b"/tmp/a".to_vec());
    let second = FdMapEntry::new(3, 0x11, OfdKind::Dir, 1, 2);
    FdMap {
        entries: vec![first, second],
    }
}

#[test]
fn empty_block_round_trip() {
    let map = FdMap::empty();
    let bytes = map.encode().expect("encode");
    assert_eq!(bytes.len(), HEADER_LEN);
    assert_eq!(&bytes[..4], &FDMAP_MAGIC);
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), FDMAP_VERSION);
    let decoded = FdMap::decode(&bytes).expect("decode");
    assert!(decoded.entries.is_empty());
}

#[test]
fn round_trip_keeps_every_field() {
    let map = sample();
    let bytes = map.encode().expect("encode");
    let decoded = FdMap::decode(&bytes).expect("decode");
    assert_eq!(decoded, map);
}

#[test]
fn string_table_is_shared() {
    let mut map = sample();
    map.entries[1].path = Some(b"/tmp/a".to_vec());
    let bytes = map.encode().expect("encode");
    // Header + two entries + one NUL-terminated string.
    assert_eq!(bytes.len(), HEADER_LEN + 2 * ENTRY_LEN + 14);
    let decoded = FdMap::decode(&bytes).expect("decode");
    assert_eq!(
        decoded.entries[1].path.as_deref(),
        Some(b"/tmp/a".as_slice())
    );
}

#[test]
fn no_path_is_marked() {
    let map = FdMap {
        entries: vec![FdMapEntry::new(1, 2, OfdKind::File, 0, 0)],
    };
    let bytes = map.encode().expect("encode");
    let offset = HEADER_LEN + 14;
    assert_eq!(
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3]
        ]),
        NO_PATH
    );
}

#[test]
fn bad_magic_is_eproto() {
    let mut bytes = sample().encode().expect("encode");
    bytes[0] = b'X';
    assert_eq!(FdMap::decode(&bytes), Err(Errno::EPROTO));
}

#[test]
fn bad_version_is_eproto() {
    let mut bytes = sample().encode().expect("encode");
    bytes[4] = 9;
    assert_eq!(FdMap::decode(&bytes), Err(Errno::EPROTO));
}

#[test]
fn truncation_is_eproto() {
    let bytes = sample().encode().expect("encode");
    assert_eq!(FdMap::decode(&bytes[..HEADER_LEN]), Err(Errno::EPROTO));
    assert_eq!(FdMap::decode(&bytes[..bytes.len() - 1]), Err(Errno::EPROTO));
    assert_eq!(FdMap::decode(&[]), Err(Errno::EPROTO));
}

#[test]
fn too_many_entries_is_e2big() {
    let map = FdMap {
        entries: (0..=u16::MAX as i32)
            .map(|fd| FdMapEntry::new(fd, 1, OfdKind::File, 0, 0))
            .collect(),
    };
    assert_eq!(map.encode(), Err(Errno::E2BIG));
}

#[test]
fn nulted_path_is_einval() {
    let mut map = FdMap::empty();
    let mut entry = FdMapEntry::new(0, 1, OfdKind::File, 0, 0);
    entry.path = Some(vec![b'a', 0, b'b']);
    map.entries.push(entry);
    assert_eq!(map.encode(), Err(Errno::EINVAL));
}

#[test]
fn transferable_kinds() {
    assert!(transferable(OfdKind::File));
    assert!(transferable(OfdKind::Dir));
    assert!(transferable(OfdKind::Tty));
    assert!(transferable(OfdKind::Socket));
    assert!(!transferable(OfdKind::Epoll));
    assert!(!transferable(OfdKind::Timer));
    assert!(!transferable(OfdKind::ProcessFd));
}

#[test]
fn entry_handle_wraps_the_number() {
    let entry = FdMapEntry::new(0, 0xabcd, OfdKind::File, 0, 0);
    assert_eq!(entry_handle(&entry).0, 0xabcd);
}
